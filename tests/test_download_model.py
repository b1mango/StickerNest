from contextlib import redirect_stderr
import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
import urllib.request
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    'download_model', Path(__file__).parents[1] / 'scripts/download_model.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

MANIFEST_URL = 'https://github.com/b1mango/StickerNest/releases/download/classifier/manifest.json'
MODEL_URL = 'https://github.com/b1mango/StickerNest/releases/download/classifier/vision.safetensors'
PACK_URL = 'https://objects.githubusercontent.com/characters.json'
MODEL_BYTES = b'clip-vision-weights'
PACK_BYTES = b'{"characters": ["sample"]}'


def manifest(model=MODEL_BYTES, pack=PACK_BYTES):
    return {
        'version': 1,
        'model': m.MODEL,
        'files': [
            {'name': 'vision.safetensors', 'url': MODEL_URL,
             'sha256': hashlib.sha256(model).hexdigest(), 'size': len(model)},
            {'name': 'characters.json', 'url': PACK_URL,
             'sha256': hashlib.sha256(pack).hexdigest(), 'size': len(pack)},
        ],
    }


class Response(io.BytesIO):
    def __init__(self, data, url, status=200, headers=None):
        super().__init__(data)
        self.url = url
        self.status = status
        self.headers = headers or {}

    def geturl(self):
        return self.url

    def getcode(self):
        return self.status


class Opener:
    def __init__(self, responses):
        self.responses = responses
        self.calls = []

    def open(self, request, timeout):
        self.calls.append((request.full_url, request.get_header('Range'), timeout))
        response = self.responses[request.full_url]
        if callable(response):
            return response(request)
        return Response(response, request.full_url)


def responses(value=None):
    value = value or manifest()
    raw = json.dumps(value, separators=(',', ':')).encode()
    return {MANIFEST_URL: raw, MODEL_URL: MODEL_BYTES, PACK_URL: PACK_BYTES}, raw


class ManifestTests(unittest.TestCase):
    def test_system_proxy_used_when_environment_has_only_no_proxy(self):
        system = {'http': 'http://127.0.0.1:7897', 'https': 'http://127.0.0.1:7897'}
        with patch.object(m.urllib.request, 'getproxies_environment', return_value={'no': 'localhost'}), \
             patch.object(m.urllib.request, 'getproxies_macosx_sysconf', return_value=system, create=True):
            self.assertEqual(m.proxies()['https'], 'http://127.0.0.1:7897')
            self.assertEqual(m.proxies()['no'], 'localhost')
        env = {'https': 'http://10.0.0.1:8080'}
        with patch.object(m.urllib.request, 'getproxies_environment', return_value=env), \
             patch.object(m.urllib.request, 'getproxies_macosx_sysconf', return_value=system, create=True):
            self.assertEqual(m.proxies(), env)

    def test_file_name_and_url_allowlist(self):
        valid = manifest()
        self.assertEqual(len(m.validate_manifest(valid)), 2)
        invalid_names = ['../vision.safetensors', 'manifest.json', 'other.bin', ['vision.safetensors']]
        for name in invalid_names:
            value = copy.deepcopy(valid)
            value['files'][0]['name'] = name
            with self.subTest(name=name), self.assertRaises(m.DownloadError):
                m.validate_manifest(value)
        duplicate = copy.deepcopy(valid)
        duplicate['files'][1]['name'] = 'vision.safetensors'
        with self.assertRaises(m.DownloadError):
            m.validate_manifest(duplicate)
        invalid_urls = [
            'http://github.com/model',
            'https://evil.com/model',
            'https://github.com.evil.com/model',
            'https://objects.githubusercontent.com.evil.com/model',
            'https://release-assets.githubusercontent.com.evil.com/model',
            'https://user:password@github.com/model',
        ]
        for url in invalid_urls:
            value = copy.deepcopy(valid)
            value['files'][0]['url'] = url
            with self.subTest(url=url), self.assertRaises(m.DownloadError):
                m.validate_manifest(value)
        # GitHub now redirects release downloads to release-assets.githubusercontent.com.
        self.assertEqual(m.check_url('https://release-assets.githubusercontent.com/github-production-release-asset/1/x'),
                         'https://release-assets.githubusercontent.com/github-production-release-asset/1/x')
        for version in [True, 1.0, 2]:
            value = copy.deepcopy(valid)
            value['version'] = version
            with self.subTest(version=version), self.assertRaises(m.DownloadError):
                m.validate_manifest(value)

    def test_manifest_url_is_validated_before_network(self):
        opener = Opener({})
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaises(m.DownloadError):
                m.download('http://github.com/manifest.json', root, opener)
        self.assertEqual(opener.calls, [])


class DownloadTests(unittest.TestCase):
    def test_download_verify_atomic_manifest_skip_and_progress(self):
        response_map, raw_manifest = responses()
        opener = Opener(response_map)
        with tempfile.TemporaryDirectory() as root:
            stderr = io.StringIO()
            with redirect_stderr(stderr):
                m.download(MANIFEST_URL, root, opener)
            destination = Path(root)
            self.assertEqual((destination / 'vision.safetensors').read_bytes(), MODEL_BYTES)
            self.assertEqual((destination / 'characters.json').read_bytes(), PACK_BYTES)
            self.assertEqual((destination / 'manifest.json').read_bytes(), raw_manifest)
            self.assertFalse(list(destination.glob('*.part')))
            lines = stderr.getvalue().splitlines()
            progress = [json.loads(line.removeprefix('PROGRESS ')) for line in lines]
            total = len(MODEL_BYTES) + len(PACK_BYTES)
            self.assertTrue(all(line.startswith('PROGRESS ') for line in lines))
            self.assertTrue(all(row['stage'] == 'downloading-model' for row in progress))
            self.assertEqual(progress[-1]['done'], total)
            self.assertEqual(progress[-1]['total'], total)
            self.assertEqual([row['done'] for row in progress], sorted(row['done'] for row in progress))
            percentages = [row['done'] * 100 // total for row in progress]
            self.assertEqual(len(percentages), len(set(percentages)))
            self.assertLessEqual(len(lines), 100)

            manifest_only = Opener({MANIFEST_URL: raw_manifest})
            m.download(MANIFEST_URL, root, manifest_only)
            self.assertEqual([call[0] for call in manifest_only.calls], [MANIFEST_URL])

    def test_existing_part_uses_range_and_verifies_content_range(self):
        response_map, _ = responses()
        offset = 7
        def resumed(request):
            self.assertEqual(request.get_header('Range'), f'bytes={offset}-')
            return Response(MODEL_BYTES[offset:], MODEL_URL, 206, {
                'Content-Range': f'bytes {offset}-{len(MODEL_BYTES) - 1}/{len(MODEL_BYTES)}',
            })
        response_map[MODEL_URL] = resumed
        opener = Opener(response_map)
        with tempfile.TemporaryDirectory() as root:
            destination = Path(root)
            (destination / 'vision.safetensors.part').write_bytes(MODEL_BYTES[:offset])
            m.download(MANIFEST_URL, root, opener)
            self.assertEqual((destination / 'vision.safetensors').read_bytes(), MODEL_BYTES)
            self.assertFalse((destination / 'vision.safetensors.part').exists())
        model_calls = [call for call in opener.calls if call[0] == MODEL_URL]
        self.assertEqual(model_calls, [(MODEL_URL, f'bytes={offset}-', 30)])

    def test_interrupted_transfer_retries_from_saved_part(self):
        response_map, _ = responses()
        offset = 5
        calls = []
        def interrupted(request):
            calls.append(request.get_header('Range'))
            if len(calls) == 1:
                return Response(MODEL_BYTES[:offset], MODEL_URL)
            return Response(MODEL_BYTES[offset:], MODEL_URL, 206, {
                'Content-Range': f'bytes {offset}-{len(MODEL_BYTES) - 1}/{len(MODEL_BYTES)}',
            })
        response_map[MODEL_URL] = interrupted
        with tempfile.TemporaryDirectory() as root:
            m.download(MANIFEST_URL, root, Opener(response_map))
            self.assertEqual((Path(root) / 'vision.safetensors').read_bytes(), MODEL_BYTES)
        self.assertEqual(calls, [None, f'bytes={offset}-'])

    def test_sha_mismatch_is_rejected_without_manifest(self):
        value = manifest()
        value['files'][0]['sha256'] = '0' * 64
        response_map, _ = responses(value)
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaisesRegex(m.DownloadError, 'SHA-256'):
                m.download(MANIFEST_URL, root, Opener(response_map))
            destination = Path(root)
            self.assertFalse((destination / 'vision.safetensors').exists())
            self.assertFalse((destination / 'vision.safetensors.part').exists())
            self.assertFalse((destination / 'manifest.json').exists())

    def test_network_failure_retries_three_times(self):
        class FailingOpener:
            calls = 0
            def open(self, request, timeout):
                self.calls += 1
                raise OSError('offline')
        opener = FailingOpener()
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaisesRegex(m.DownloadError, '已重试 3 次'):
                m.download(MANIFEST_URL, root, opener)
        self.assertEqual(opener.calls, 4)

    def test_default_opener_does_not_disable_system_proxy(self):
        with patch.object(m.urllib.request, 'build_opener', wraps=m.urllib.request.build_opener) as build:
            with patch.object(m.urllib.request.OpenerDirector, 'open', side_effect=OSError('offline')):
                with tempfile.TemporaryDirectory() as root:
                    with self.assertRaises(m.DownloadError):
                        m.download(MANIFEST_URL, root)
        handlers = build.call_args.args
        self.assertFalse(any(isinstance(handler, m.urllib.request.ProxyHandler)
                             and handler.proxies == {} for handler in handlers))

    def test_main_reports_chinese_error_as_last_stderr_line(self):
        stderr = io.StringIO()
        with patch.object(m, 'download', side_effect=m.DownloadError('校验值不一致')):
            with patch.object(m.sys, 'argv', ['download_model.py', MANIFEST_URL, '/unused']):
                with redirect_stderr(stderr):
                    self.assertEqual(m.main(), 1)
        self.assertIn('形象识别下载失败：校验值不一致', stderr.getvalue().splitlines()[-1])


if __name__ == '__main__':
    unittest.main()
