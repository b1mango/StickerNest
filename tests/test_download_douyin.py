import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
import urllib.request
from unittest.mock import patch
from PIL import Image

spec = importlib.util.spec_from_file_location('download', Path(__file__).parents[1] / 'scripts/download_douyin.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
URL = 'https://p3-im-emoticon-sign.byteimg.com/resource?secret=do-not-log'


def item(i='123'):
    return {'id_str': i, 'animate_url': {'url_list': [URL]}, 'static_url': {'url_list': [URL + 'static']}}


def gif():
    stream = io.BytesIO()
    Image.new('RGB', (2, 2), 'red').save(stream, format='GIF', save_all=True,
        append_images=[Image.new('RGB', (2, 2), 'blue')], duration=100, loop=0)
    return stream.getvalue()


class DownloadTests(unittest.TestCase):
    def test_download_dedup_resume_no_secrets(self):
        data = gif()
        with tempfile.TemporaryDirectory() as root:
            report = m.collect([item(), item('124')], root, lambda url: data)
            self.assertEqual(report['counts']['saved_resources'], 2)
            self.assertEqual(report['counts']['unique_contents'], 1)
            self.assertEqual(report['items'][0]['frames'], 2)
            self.assertEqual(report['items'][0]['resource_role'], 'animate_url')
            self.assertNotIn('secret', json.dumps(report))
            again = m.collect([item(), item('124')], root, lambda url: self.fail('resume used network'))
            self.assertTrue(all(r['resumed'] for r in again['items']))
            p = Path(root) / report['items'][0]['file']
            p.write_bytes(b'changed')
            damaged = m.collect([item()], root, lambda url: self.fail('overwrote damaged original'))
            self.assertEqual(damaged['items'][0]['error'], 'existing_content_mismatch')
            self.assertEqual(p.read_bytes(), b'changed')

    def test_url_boundary(self):
        for url in ['http://p3-im-emoticon-sign.byteimg.com/a', 'https://evil.byteimg.com/a',
                    'https://p3-im-emoticon-sign.byteimg.com.evil.com',
                    'https://user:password@p3-im-emoticon-sign.byteimg.com',
                    'https://p3-im-emoticon-sign.byteimg.com:123/a']:
            with self.assertRaises(m.DownloadError):
                m.check_url(url)
        request = urllib.request.Request(URL)
        with self.assertRaises(m.DownloadError):
            m.SameHostRedirect().redirect_request(request, None, 302, '', {},
                'https://p26-im-emoticon-sign.byteimg.com/a')

    def test_bad_ids_and_duplicates(self):
        with tempfile.TemporaryDirectory() as root:
            for items in [[item('../bad')], [item(), item()]]:
                with self.assertRaises(m.DownloadError):
                    m.collect(items, root)

    def test_unknown_bytes_preserved(self):
        with tempfile.TemporaryDirectory() as root:
            report = m.collect([item()], root, lambda url: b'unknown-format')
            row = report['items'][0]
            self.assertEqual(row['status'], 'pending_inspection')
            self.assertEqual(Path(root, row['file']).read_bytes(), b'unknown-format')
            self.assertTrue(row['file'].endswith('.bin'))

    def test_size_and_decode_limits(self):
        with tempfile.TemporaryDirectory() as root, patch.object(m, 'MAX_BYTES', 5):
            report = m.collect([item()], root, lambda url: b'123456')
            self.assertEqual(report['items'][0]['error'], 'resource_too_large')
        stream = io.BytesIO()
        Image.new('RGB', (4097, 1)).save(stream, format='PNG')
        with self.assertRaises(m.DownloadError):
            m.inspect_bytes(stream.getvalue())

    def test_fallback_static_and_sanitized_error(self):
        sample = item()
        sample['animate_url']['url_list'] = []
        with tempfile.TemporaryDirectory() as root:
            report = m.collect([sample], root, lambda url: gif())
            self.assertEqual(report['items'][0]['resource_role'], 'static_url')
            sample['static_url']['url_list'] = ['https://evil.com/?secret=xyz']
            report = m.collect([sample], Path(root) / 'other')
            self.assertEqual(report['items'][0]['error'], 'untrusted_resource_url')
            self.assertNotIn('xyz', json.dumps(report))

    def test_originals_symlink_rejected(self):
        with tempfile.TemporaryDirectory() as root:
            destination = Path(root) / 'download'
            destination.mkdir()
            (destination / 'originals').symlink_to(root, target_is_directory=True)
            with self.assertRaises(m.DownloadError):
                m.collect([item()], destination, lambda url: gif())

class NetworkTests(unittest.TestCase):
    def test_stream_limit_without_content_length(self):
        class Response(io.BytesIO):
            headers = {}
            def geturl(self):
                return URL
        class Opener:
            def open(self, request, timeout):
                return Response(b'123456')
        with patch.object(m, 'MAX_BYTES', 5):
            with self.assertRaisesRegex(m.DownloadError, 'resource_too_large'):
                m.fetch(URL, Opener())

    def test_network_failure_has_finite_retry_and_no_secret(self):
        class Opener:
            calls = 0
            def open(self, request, timeout):
                self.calls += 1
                raise OSError(URL)
        opener = Opener()
        with patch.object(m.time, 'sleep'):
            with self.assertRaisesRegex(m.DownloadError, '^network_failure$'):
                m.fetch(URL, opener)
        self.assertEqual(opener.calls, 2)

    def test_missing_original_can_be_recovered(self):
        with tempfile.TemporaryDirectory() as root:
            report = m.collect([item()], root, lambda url: gif())
            Path(root, report['items'][0]['file']).unlink()
            report = m.collect([item()], root, lambda url: gif())
            self.assertEqual(report['counts']['verified_resources'], 1)
            self.assertFalse(report['items'][0]['resumed'])


class PreservationTests(unittest.TestCase):
    def test_oversized_image_is_preserved_for_inspection(self):
        stream = io.BytesIO()
        Image.new('RGB', (4097, 1)).save(stream, format='PNG')
        with tempfile.TemporaryDirectory() as root:
            report = m.collect([item()], root, lambda url: stream.getvalue())
            row = report['items'][0]
            self.assertEqual(row['inspection_error'], 'image_limits_exceeded')
            self.assertEqual(Path(root, row['file']).read_bytes(), stream.getvalue())

    def test_identity_change_redownloads_and_keeps_old(self):
        with tempfile.TemporaryDirectory() as root:
            first = item()
            first['hash'] = 'one'
            old = m.collect([first], root, lambda url: gif())['items'][0]
            first['hash'] = 'two'
            current = m.collect([first], root, lambda url: b'new resource')['items'][0]
            self.assertNotEqual(old['sha256'], current['sha256'])
            self.assertTrue(Path(root, old['file']).exists())

    def test_checkpoint_keeps_unvisited_previous_success(self):
        with tempfile.TemporaryDirectory() as root:
            m.collect([item(), item('124')], root, lambda url: gif())
            changed = item('124')
            changed['hash'] = 'new'
            def stop(url):
                raise KeyboardInterrupt()
            with self.assertRaises(KeyboardInterrupt):
                m.collect([item(), changed], root, stop)
            checkpoint = json.loads(Path(root, 'report.json').read_text())
            self.assertEqual(len(checkpoint['items']), 2)


if __name__ == '__main__':
    unittest.main()
