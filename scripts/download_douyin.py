#!/usr/bin/env python3
"""Download a locally verified Douyin sticker list; never logs signed URLs.

Requires Pillow for decoding. No login, cookies or platform list requests.
Usage: python3 scripts/download_douyin.py INPUT.json OUTPUT_DIRECTORY
Originals and report.json stay local. Interrupted runs can be resumed.
"""
import argparse
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import re
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import warnings

from PIL import Image, UnidentifiedImageError

MAX_BYTES = 20 * 1024 * 1024
HOSTS = {'p3-im-emoticon-sign.byteimg.com', 'p26-im-emoticon-sign.byteimg.com'}
EXTENSIONS = {'PNG': 'png', 'JPEG': 'jpg', 'GIF': 'gif', 'WEBP': 'webp'}


class DownloadError(Exception):
    """Only fixed, credential-free error codes cross this boundary."""


def check_url(url):
    try:
        p = urllib.parse.urlsplit(url)
        valid = (p.scheme == 'https' and p.hostname in HOSTS and
                 p.port in (None, 443) and not p.username and not p.password)
    except (ValueError, TypeError):
        valid = False
    if not valid:
        raise DownloadError('untrusted_resource_url')
    return p


class SameHostRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        old, new = check_url(req.full_url), check_url(newurl)
        if old.hostname != new.hostname:
            raise DownloadError('cross_host_redirect')
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def fetch(url, opener=None):
    check_url(url)
    opener = opener or urllib.request.build_opener(SameHostRedirect())
    for attempt in range(2):
        try:
            request = urllib.request.Request(url, headers={'User-Agent': 'StickerNest/0.1'})
            started = time.monotonic()
            with opener.open(request, timeout=15) as response:
                check_url(response.geturl())
                if urllib.parse.urlsplit(response.geturl()).hostname != check_url(url).hostname:
                    raise DownloadError('cross_host_redirect')
                length = response.headers.get('Content-Length')
                if length and int(length) > MAX_BYTES:
                    raise DownloadError('resource_too_large')
                chunks, size = [], 0
                while True:
                    chunk = response.read(min(65536, MAX_BYTES + 1 - size))
                    if time.monotonic() - started > 60:
                        raise DownloadError('download_deadline')
                    if not chunk:
                        break
                    size += len(chunk)
                    if size > MAX_BYTES:
                        raise DownloadError('resource_too_large')
                    chunks.append(chunk)
                if not size:
                    raise DownloadError('empty_resource')
                return b''.join(chunks)
        except DownloadError:
            raise
        except urllib.error.HTTPError as exc:
            if attempt == 0 and (exc.code == 429 or exc.code >= 500):
                time.sleep(1)
                continue
            raise DownloadError('http_' + str(exc.code)) from None
        except (OSError, ValueError):
            if attempt == 0:
                time.sleep(1)
                continue
            raise DownloadError('network_failure') from None


def inspect_bytes(data):
    try:
        with warnings.catch_warnings():
            warnings.simplefilter('error', Image.DecompressionBombWarning)
            with Image.open(io.BytesIO(data)) as im:
                fmt = im.format
                if fmt not in EXTENSIONS:
                    return {'format': fmt, 'frames': None, 'extension': 'bin', 'status': 'pending_inspection'}
                width, height = im.size
                frames = getattr(im, 'n_frames', 1)
                if (width > 4096 or height > 4096 or frames > 1000 or
                        width * height * frames > 100_000_000):
                    raise DownloadError('image_limits_exceeded')
                for index in range(frames):
                    im.seek(index)
                    im.load()
                return {'format': fmt, 'frames': frames, 'width': width, 'height': height,
                        'extension': EXTENSIONS[fmt], 'status': 'verified'}
    except UnidentifiedImageError:
        return {'format': None, 'frames': None, 'extension': 'bin', 'status': 'pending_inspection'}
    except DownloadError:
        raise
    except (OSError, ValueError, EOFError, Image.DecompressionBombError, Image.DecompressionBombWarning):
        raise DownloadError('invalid_image') from None


def atomic_json(path, value):
    fd, temp = tempfile.mkstemp(prefix='.report-', dir=path.parent)
    try:
        with os.fdopen(fd, 'w') as file:
            json.dump(value, file, ensure_ascii=False, indent=2)
            file.flush()
            os.fsync(file.fileno())
        os.replace(temp, path)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


def read_original(path):
    if path.is_symlink() or not path.is_file():
        raise DownloadError('invalid_existing_file')
    if path.stat().st_size > MAX_BYTES:
        raise DownloadError('resource_too_large')
    return path.read_bytes()


def persist_original(directory, data, extension):
    digest = hashlib.sha256(data).hexdigest()
    name = digest + '.' + extension
    path = directory / name
    if path.exists() or path.is_symlink():
        if read_original(path) != data:
            raise DownloadError('existing_content_mismatch')
    else:
        fd, temporary = tempfile.mkstemp(prefix='.download-', dir=directory)
        try:
            with os.fdopen(fd, 'wb') as file:
                file.write(data)
                file.flush()
                os.fsync(file.fileno())
            # Hard link is atomic and refuses to overwrite any existing file.
            os.link(temporary, path)
        finally:
            os.unlink(temporary)
    return name, digest


def select_resource(item):
    for role in ('animate_url', 'static_url'):
        urls = (item.get(role) or {}).get('url_list') or []
        if urls:
            return role, list(dict.fromkeys(urls))
    raise DownloadError('no_resource_url')


def collect(items, destination, fetcher=fetch):
    if not isinstance(items, list):
        raise DownloadError('invalid_list')
    ids = [item.get('id_str') if isinstance(item, dict) else None for item in items]
    if any(not isinstance(i, str) or not re.fullmatch(r'[0-9]{1,40}', i) for i in ids):
        raise DownloadError('invalid_sticker_id')
    if len(set(ids)) != len(ids):
        raise DownloadError('duplicate_sticker_id')
    destination = Path(destination)
    if destination.is_symlink():
        raise DownloadError('symlink_destination')
    destination.mkdir(parents=True, exist_ok=True)
    if (destination / '.lock').is_symlink():
        raise DownloadError('symlink_lock')
    with open(destination / '.lock', 'a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise DownloadError('collection_already_running') from None
        return _collect_locked(items, destination, fetcher)


def _collect_locked(items, destination, fetcher):
    originals = destination / 'originals'
    if originals.is_symlink():
        raise DownloadError('symlink_originals')
    originals.mkdir(exist_ok=True)
    report_path = destination / 'report.json'
    previous = {}
    if report_path.is_symlink():
        raise DownloadError('symlink_report')
    if report_path.exists():
        previous = {row['id']: row for row in json.loads(report_path.read_text()).get('items', [])}
    ids = {item['id_str'] for item in items}
    checkpoint = {key: value for key, value in previous.items() if key in ids}
    report = {'schema_version': 1, 'expected_items': len(items), 'items': [], 'counts': {}}
    for item in items:
        row = {'id': item['id_str']}
        try:
            role, urls = select_resource(item)
            row['resource_role'] = role
            identity = json.dumps([item.get('hash'), (item.get(role) or {}).get('uri')], sort_keys=True)
            row['resource_identity'] = hashlib.sha256(identity.encode()).hexdigest()
            old = previous.get(row['id'], {})
            digest = old.get('sha256', '')
            extension = old.get('extension', '')
            if (old.get('resource_role') == role
                    and old.get('resource_identity') == row['resource_identity'] and re.fullmatch('[a-f0-9]{64}', digest)
                    and extension in {*EXTENSIONS.values(), 'bin'}
                    and (originals / (digest + '.' + extension)).exists()):
                data = read_original(originals / (digest + '.' + extension))
                if hashlib.sha256(data).hexdigest() != digest:
                    raise DownloadError('existing_content_mismatch')
                resumed = True
            else:
                data, resumed = None, False
                for url in urls[:4]:
                    try:
                        check_url(url)
                        data = fetcher(url)
                        break
                    except DownloadError as exc:
                        last_error = exc
                if data is None:
                    raise last_error
            if len(data) > MAX_BYTES:
                raise DownloadError('resource_too_large')
            try:
                info = inspect_bytes(data)
            except DownloadError as exc:
                info = {'format': None, 'frames': None, 'extension': 'bin',
                        'status': 'pending_inspection', 'inspection_error': str(exc)}
            name, digest = persist_original(originals, data, info['extension'])
            row.update(info, file='originals/' + name, sha256=digest, size=len(data), resumed=resumed)
        except DownloadError as exc:
            row.update(status='failed', error=str(exc))
        except (OSError, ValueError, TypeError, AttributeError):
            row.update(status='failed', error='local_or_input_failure')
        checkpoint[row['id']] = row
        report['items'] = [checkpoint[item['id_str']] for item in items if item['id_str'] in checkpoint]
        saved = [r for r in report['items'] if r.get('sha256')]
        report['counts'] = {'collection_items': len(items), 'processed_items': len(report['items']),
                            'saved_resources': len(saved), 'unique_contents': len({r['sha256'] for r in saved}),
                            'verified_resources': sum(r['status'] == 'verified' for r in saved),
                            'pending_inspection': sum(r['status'] == 'pending_inspection' for r in saved),
                            'failed': sum(r['status'] == 'failed' for r in report['items'])}
        atomic_json(report_path, report)
    if not items:
        atomic_json(report_path, report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('input', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    try:
        report = collect(json.loads(args.input.read_text()), args.destination)
        print(json.dumps(report['counts'], ensure_ascii=False))
        return 1 if report['counts'].get('failed') else 0
    except DownloadError as exc:
        print('采集停止：' + str(exc))
        return 1
    except (OSError, ValueError, TypeError):
        print('采集停止：本地输入或文件操作失败')
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
