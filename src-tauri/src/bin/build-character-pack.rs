use serde::Deserialize;
use stickernest_lib::{
    classify::{Character, CharacterPack, Thresholds, BATCH_SIZE},
    vision::VisionModel,
};
use std::{ffi::OsStr, path::PathBuf};

const USAGE: &str = "用法：\n  build-character-pack --model <safetensors> --spec <spec.json> --out <characters.json>\n  build-character-pack --model <safetensors> --embed <图片>...";

struct Arguments {
    model: PathBuf,
    spec: Option<PathBuf>,
    out: Option<PathBuf>,
    embed: Vec<PathBuf>,
}

fn arguments() -> Result<Arguments, String> {
    let mut model = None;
    let mut spec = None;
    let mut out = None;
    let mut embed = Vec::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(argument) = args.next() {
        if argument == OsStr::new("--model") {
            model = Some(PathBuf::from(args.next().ok_or(USAGE)?));
        } else if argument == OsStr::new("--spec") {
            spec = Some(PathBuf::from(args.next().ok_or(USAGE)?));
        } else if argument == OsStr::new("--out") {
            out = Some(PathBuf::from(args.next().ok_or(USAGE)?));
        } else if argument == OsStr::new("--embed") {
            embed.extend(args.map(PathBuf::from));
            break;
        } else {
            return Err(USAGE.into());
        }
    }
    let model = model.ok_or(USAGE)?;
    if embed.is_empty() && (spec.is_none() || out.is_none()) {
        return Err(USAGE.into());
    }
    if !embed.is_empty() && (spec.is_some() || out.is_some()) {
        return Err("--embed 不能与 --spec、--out 同时使用".into());
    }
    Ok(Arguments { model, spec, out, embed })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Spec {
    thresholds: Option<Thresholds>,
    characters: Vec<SpecCharacter>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpecCharacter {
    name: String,
    images: Vec<PathBuf>,
    #[serde(default)]
    thresholds: Option<Thresholds>,
}

fn embed_paths(model: &VisionModel, paths: &[PathBuf]) -> Result<Vec<Vec<f32>>, String> {
    let mut vectors = Vec::with_capacity(paths.len());
    for batch in paths.chunks(BATCH_SIZE) {
        vectors.extend(model.embed_paths(batch)?);
    }
    Ok(vectors)
}

fn run() -> Result<(), String> {
    let arguments = arguments()?;
    let model = VisionModel::load(&arguments.model)?;
    if !arguments.embed.is_empty() {
        let vectors = embed_paths(&model, &arguments.embed)?;
        for (path, vector) in arguments.embed.iter().zip(vectors) {
            let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
            println!("{}\t{:.8?}\tnorm={norm:.8}", path.display(), &vector[..8]);
        }
        return Ok(());
    }

    let spec_path = arguments.spec.ok_or(USAGE)?;
    let out = arguments.out.ok_or(USAGE)?;
    let spec: Spec = serde_json::from_slice(
        &std::fs::read(&spec_path).map_err(|e| format!("无法读取形象样例配置：{e}"))?,
    ).map_err(|e| format!("形象样例配置无效：{e}"))?;
    let mut characters = Vec::with_capacity(spec.characters.len());
    for character in spec.characters {
        let samples = embed_paths(&model, &character.images)?;
        characters.push(Character { name: character.name, samples, thresholds: character.thresholds });
    }
    let pack = CharacterPack::new(spec.thresholds.unwrap_or_default(), characters)?;
    std::fs::write(&out, pack.to_json()?)
        .map_err(|e| format!("无法写入形象包 {}：{e}", out.display()))?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("形象包构建失败：{error}");
        std::process::exit(1);
    }
}
