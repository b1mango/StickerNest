use candle_core::{DType, Device, IndexOp, Tensor, D};
use candle_nn::{Conv2d, Conv2dConfig, LayerNorm, Linear, Module, VarBuilder};
use image::{DynamicImage, ImageReader, RgbImage};
use std::{io::Cursor, path::{Path, PathBuf}, sync::Mutex};

pub const DIMENSION: usize = 512;
pub const IMAGE_SIZE: u32 = 224;
const MEAN: [f32; 3] = [0.48145466, 0.4578275, 0.40821073];
const STD: [f32; 3] = [0.26862954, 0.26130258, 0.27577711];

static MODEL_CACHE: Mutex<Option<VisionModel>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HiddenAct {
    Gelu,
    QuickGelu,
}

impl Module for HiddenAct {
    fn forward(&self, xs: &Tensor) -> candle_core::Result<Tensor> {
        match self {
            Self::Gelu => xs.gelu_erf(),
            Self::QuickGelu => xs * candle_nn::ops::sigmoid(&(xs * 1.702f64)?)?,
        }
    }
}

#[derive(Clone, Copy)]
struct VisionConfig {
    embed_dim: usize,
    intermediate_size: usize,
    layers: usize,
    heads: usize,
    patch_size: usize,
    projection_dim: usize,
    activation: HiddenAct,
}

impl VisionConfig {
    const fn laion(activation: HiddenAct) -> Self {
        Self {
            embed_dim: 768,
            intermediate_size: 3072,
            layers: 12,
            heads: 12,
            patch_size: 32,
            projection_dim: DIMENSION,
            activation,
        }
    }
}

struct VisionEmbeddings {
    class_embedding: Tensor,
    patch_embedding: Conv2d,
    position_embedding: Tensor,
    embed_dim: usize,
}

impl VisionEmbeddings {
    fn new(vb: VarBuilder, config: &VisionConfig) -> candle_core::Result<Self> {
        let class_embedding = vb.get(config.embed_dim, "class_embedding")?;
        let patch_embedding = candle_nn::conv2d_no_bias(
            3,
            config.embed_dim,
            config.patch_size,
            Conv2dConfig { stride: config.patch_size, ..Default::default() },
            vb.pp("patch_embedding"),
        )?;
        let position_embedding = vb.pp("position_embedding").get(
            ((IMAGE_SIZE as usize / config.patch_size).pow(2) + 1, config.embed_dim),
            "weight",
        )?;
        Ok(Self { class_embedding, patch_embedding, position_embedding, embed_dim: config.embed_dim })
    }

    fn forward(&self, pixels: &Tensor) -> candle_core::Result<Tensor> {
        let batch = pixels.dim(0)?;
        let patches = self.patch_embedding.forward(pixels)?.flatten_from(2)?.transpose(1, 2)?;
        let class = self.class_embedding.expand((batch, 1, self.embed_dim))?;
        Tensor::cat(&[class, patches], 1)?.broadcast_add(&self.position_embedding)
    }
}

struct Attention {
    q_proj: Linear,
    k_proj: Linear,
    v_proj: Linear,
    out_proj: Linear,
    heads: usize,
    head_dim: usize,
    scale: f64,
}

impl Attention {
    fn new(vb: VarBuilder, config: &VisionConfig) -> candle_core::Result<Self> {
        Ok(Self {
            q_proj: candle_nn::linear(config.embed_dim, config.embed_dim, vb.pp("q_proj"))?,
            k_proj: candle_nn::linear(config.embed_dim, config.embed_dim, vb.pp("k_proj"))?,
            v_proj: candle_nn::linear(config.embed_dim, config.embed_dim, vb.pp("v_proj"))?,
            out_proj: candle_nn::linear(config.embed_dim, config.embed_dim, vb.pp("out_proj"))?,
            heads: config.heads,
            head_dim: config.embed_dim / config.heads,
            scale: 1.0 / ((config.embed_dim / config.heads) as f64).sqrt(),
        })
    }

    fn split_heads(&self, xs: &Tensor, batch: usize, sequence: usize) -> candle_core::Result<Tensor> {
        xs.reshape((batch, sequence, self.heads, self.head_dim))?
            .transpose(1, 2)?
            .contiguous()?
            .reshape((batch * self.heads, sequence, self.head_dim))
    }

    fn forward(&self, xs: &Tensor) -> candle_core::Result<Tensor> {
        let (batch, sequence, embed_dim) = xs.dims3()?;
        let query = self.split_heads(&(self.q_proj.forward(xs)? * self.scale)?, batch, sequence)?;
        let key = self.split_heads(&self.k_proj.forward(xs)?, batch, sequence)?;
        let value = self.split_heads(&self.v_proj.forward(xs)?, batch, sequence)?;
        let weights = candle_nn::ops::softmax(&query.matmul(&key.transpose(1, 2)?)?, D::Minus1)?;
        let output = weights.matmul(&value)?
            .reshape((batch, self.heads, sequence, self.head_dim))?
            .transpose(1, 2)?
            .reshape((batch, sequence, embed_dim))?;
        self.out_proj.forward(&output)
    }
}

struct EncoderLayer {
    attention: Attention,
    layer_norm1: LayerNorm,
    layer_norm2: LayerNorm,
    fc1: Linear,
    fc2: Linear,
    activation: HiddenAct,
}

impl EncoderLayer {
    fn new(vb: VarBuilder, config: &VisionConfig) -> candle_core::Result<Self> {
        Ok(Self {
            attention: Attention::new(vb.pp("self_attn"), config)?,
            layer_norm1: candle_nn::layer_norm(config.embed_dim, 1e-5, vb.pp("layer_norm1"))?,
            layer_norm2: candle_nn::layer_norm(config.embed_dim, 1e-5, vb.pp("layer_norm2"))?,
            fc1: candle_nn::linear(config.embed_dim, config.intermediate_size, vb.pp("mlp.fc1"))?,
            fc2: candle_nn::linear(config.intermediate_size, config.embed_dim, vb.pp("mlp.fc2"))?,
            activation: config.activation,
        })
    }

    fn forward(&self, xs: &Tensor) -> candle_core::Result<Tensor> {
        let attended = self.attention.forward(&self.layer_norm1.forward(xs)?)?;
        let xs = (xs + attended)?;
        let mlp = self.fc2.forward(
            &self.activation.forward(&self.fc1.forward(&self.layer_norm2.forward(&xs)?)?)?,
        )?;
        xs + mlp
    }
}

pub struct VisionModel {
    embeddings: VisionEmbeddings,
    pre_layrnorm: LayerNorm,
    layers: Vec<EncoderLayer>,
    post_layernorm: LayerNorm,
    visual_projection: Linear,
}

impl VisionModel {
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::load_with_activation(path, HiddenAct::Gelu)
    }

    pub fn load_with_activation(path: &Path, activation: HiddenAct) -> Result<Self, String> {
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[path], DType::F32, &Device::Cpu) }
            .map_err(|e| format!("无法加载形象识别权重：{e}"))?;
        Self::from_var_builder(vb, &VisionConfig::laion(activation))
            .map_err(|e| format!("形象识别权重结构无效：{e}"))
    }

    fn from_var_builder(vb: VarBuilder, config: &VisionConfig) -> candle_core::Result<Self> {
        let vision = vb.pp("vision_model");
        let mut layers = Vec::with_capacity(config.layers);
        for index in 0..config.layers {
            layers.push(EncoderLayer::new(
                vision.pp(format!("encoder.layers.{index}")),
                config,
            )?);
        }
        Ok(Self {
            embeddings: VisionEmbeddings::new(vision.pp("embeddings"), config)?,
            pre_layrnorm: candle_nn::layer_norm(config.embed_dim, 1e-5, vision.pp("pre_layrnorm"))?,
            layers,
            post_layernorm: candle_nn::layer_norm(config.embed_dim, 1e-5, vision.pp("post_layernorm"))?,
            visual_projection: candle_nn::linear_no_bias(
                config.embed_dim,
                config.projection_dim,
                vb.pp("visual_projection"),
            )?,
        })
    }

    fn forward(&self, pixels: &Tensor) -> candle_core::Result<Vec<Vec<f32>>> {
        let mut hidden = self.embeddings.forward(pixels)?.apply(&self.pre_layrnorm)?;
        for layer in &self.layers {
            hidden = layer.forward(&hidden)?;
        }
        let cls = hidden.i((.., 0, ..))?.apply(&self.post_layernorm)?;
        let features = cls.apply(&self.visual_projection)?;
        let norms = features.sqr()?.sum_keepdim(D::Minus1)?.sqrt()?;
        features.broadcast_div(&norms)?.to_vec2::<f32>()
    }

    pub fn embed_images(&self, images: &[DynamicImage]) -> Result<Vec<Vec<f32>>, String> {
        let tensors = images.iter().map(preprocess).collect::<Result<Vec<_>, _>>()?;
        let batch = Tensor::stack(&tensors, 0).map_err(|e| e.to_string())?;
        self.forward(&batch).map_err(|e| format!("形象识别推理失败：{e}"))
    }

    pub fn embed_paths(&self, paths: &[PathBuf]) -> Result<Vec<Vec<f32>>, String> {
        let images = paths.iter().map(|path| decode_first_frame(path)).collect::<Result<Vec<_>, _>>()?;
        self.embed_images(&images)
    }
}

fn decode_first_frame(path: &Path) -> Result<DynamicImage, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("无法读取图片 {}：{e}", path.display()))?;
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("无法识别图片 {}：{e}", path.display()))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|e| format!("无法解码图片 {}：{e}", path.display()))
}

fn composite_white(image: &DynamicImage) -> RgbImage {
    let rgba = image.to_rgba8();
    let mut rgb = RgbImage::new(rgba.width(), rgba.height());
    for (target, source) in rgb.pixels_mut().zip(rgba.pixels()) {
        let [r, g, b, a] = source.0;
        let alpha = a as u32;
        for (channel, value) in target.0.iter_mut().zip([r, g, b]) {
            *channel = ((value as u32 * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        }
    }
    rgb
}

fn resize_and_crop(image: &RgbImage) -> RgbImage {
    let (width, height) = image.dimensions();
    let shortest = width.min(height) as u64;
    let resized_width = ((width as u64 * IMAGE_SIZE as u64 + shortest / 2) / shortest) as u32;
    let resized_height = ((height as u64 * IMAGE_SIZE as u64 + shortest / 2) / shortest) as u32;
    let resized = image::imageops::resize(
        image,
        resized_width,
        resized_height,
        image::imageops::FilterType::CatmullRom,
    );
    image::imageops::crop_imm(
        &resized,
        (resized_width - IMAGE_SIZE) / 2,
        (resized_height - IMAGE_SIZE) / 2,
        IMAGE_SIZE,
        IMAGE_SIZE,
    ).to_image()
}

pub fn preprocess(image: &DynamicImage) -> Result<Tensor, String> {
    if image.width() == 0 || image.height() == 0 {
        return Err("图片尺寸不能为空".into());
    }
    let rgb = resize_and_crop(&composite_white(image));
    let area = (IMAGE_SIZE * IMAGE_SIZE) as usize;
    let mut data = vec![0f32; 3 * area];
    for (index, pixel) in rgb.pixels().enumerate() {
        for channel in 0..3 {
            data[channel * area + index] =
                (pixel[channel] as f32 / 255.0 - MEAN[channel]) / STD[channel];
        }
    }
    Tensor::from_vec(data, (3, IMAGE_SIZE as usize, IMAGE_SIZE as usize), &Device::Cpu)
        .map_err(|e| format!("无法准备图片张量：{e}"))
}

pub fn with_cached_model<T>(
    path: &Path,
    operation: impl FnOnce(&VisionModel) -> Result<T, String>,
) -> Result<T, String> {
    let mut cached = MODEL_CACHE.lock().map_err(|_| "形象识别模型忙，请稍后再试")?;
    if cached.is_none() {
        *cached = Some(VisionModel::load(path)?);
    }
    operation(cached.as_ref().unwrap())
}

pub fn clear_model_cache() -> Result<(), String> {
    *MODEL_CACHE.lock().map_err(|_| "形象识别模型忙，请稍后再试")? = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn preprocessing_composites_transparency_over_white() {
        let image = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2, 1, Rgba([255, 0, 0, 0]),
        ));
        assert_eq!(composite_white(&image).get_pixel(0, 0).0, [255, 255, 255]);
        let image = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2, 1, Rgba([10, 20, 30, 255]),
        ));
        assert_eq!(composite_white(&image).get_pixel(0, 0).0, [10, 20, 30]);
        let image = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2, 1, Rgba([0, 0, 0, 128]),
        ));
        assert_eq!(composite_white(&image).get_pixel(0, 0).0, [127, 127, 127]);
    }

    #[test]
    fn preprocessing_resizes_shortest_edge_and_center_crops() {
        let mut image = RgbImage::from_pixel(50, 100, image::Rgb([1, 2, 3]));
        for y in 0..100 {
            for x in 0..50 {
                image.put_pixel(x, y, image::Rgb([x as u8, y as u8, 0]));
            }
        }
        let resized = resize_and_crop(&image);
        assert_eq!(resized.dimensions(), (224, 224));
        let source = image::imageops::resize(
            &image, 224, 448, image::imageops::FilterType::CatmullRom,
        );
        let expected = image::imageops::crop_imm(&source, 0, 112, 224, 224).to_image();
        assert_eq!(resized, expected);
    }

    #[test]
    fn preprocessing_normalizes_nchw_values() {
        let white = DynamicImage::ImageRgb8(RgbImage::from_pixel(20, 30, image::Rgb([255, 255, 255])));
        let tensor = preprocess(&white).unwrap();
        assert_eq!(tensor.dims(), &[3, 224, 224]);
        let values = tensor.to_vec3::<f32>().unwrap();
        for channel in 0..3 {
            let expected = (1.0 - MEAN[channel]) / STD[channel];
            assert!((values[channel][0][0] - expected).abs() < 1e-6);
            assert!((values[channel][223][223] - expected).abs() < 1e-6);
        }
        let black = DynamicImage::ImageRgb8(RgbImage::from_pixel(20, 30, image::Rgb([0, 0, 0])));
        let values = preprocess(&black).unwrap().to_vec3::<f32>().unwrap();
        for channel in 0..3 {
            let expected = -MEAN[channel] / STD[channel];
            assert!((values[channel][100][100] - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn both_clip_activations_are_available() {
        let input = Tensor::new(&[1f32], &Device::Cpu).unwrap();
        let gelu = HiddenAct::Gelu.forward(&input).unwrap().to_vec1::<f32>().unwrap()[0];
        let quick = HiddenAct::QuickGelu.forward(&input).unwrap().to_vec1::<f32>().unwrap()[0];
        assert!((gelu - 0.8413447).abs() < 1e-5);
        assert!((quick - 0.84579575).abs() < 1e-6);
    }

    #[test]
    #[ignore = "需要 output/model-build/model.safetensors 真实权重"]
    fn real_weights_embed_two_generated_images() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../output/model-build/model.safetensors");
        if !path.is_file() {
            return;
        }
        let model = VisionModel::load(&path).unwrap();
        let red = DynamicImage::ImageRgb8(RgbImage::from_pixel(96, 64, image::Rgb([220, 30, 30])));
        let blue = DynamicImage::ImageRgb8(RgbImage::from_pixel(64, 96, image::Rgb([30, 30, 220])));
        let started = std::time::Instant::now();
        let vectors = model.embed_images(&[red, blue]).unwrap();
        let elapsed = started.elapsed();
        for (index, vector) in vectors.iter().enumerate() {
            let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
            println!("image={index} first8={:?} norm={norm:.6}", &vector[..8]);
            assert_eq!(vector.len(), DIMENSION);
            assert!((norm - 1.0).abs() < 1e-4);
        }
        println!("batch=2 elapsed={elapsed:?} per_image={:?}", elapsed / 2);
    }
}
