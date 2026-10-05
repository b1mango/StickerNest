#!/usr/bin/env python3
"""Download a WeChat sticker URL manifest; originals and report stay local.

Usage: python3 scripts/download_wechat.py MANIFEST.txt OUTPUT_DIRECTORY
MANIFEST is the plain URL list produced by wxemoticon (one URL per line).
Items keep file order as their collection id: 1..N. Interrupted runs resume.
No login, cookies or account data is touched; every URL is host-checked
against the WeChat emoticon CDN allowlist before downloading.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

MAX_BYTES = 20 * 1024 * 1024
ALLOWED_SUFFIXES = ('.qpic.cn', '.qlogo.cn')
EXTENSIONS = {'PNG': 'png', 'JPEG': 'jpg', 'GIF': 'gif', 'WEBP': 'webp'}
MAGIC = [
    (b'\x89PNG\r\n\x1a\n', 'PNG'),
    (b'\xff\xd8\xff', 'JPEG'),
    (b'GIF8', 'GIF'),
    (b'RIFF', 'WEBP'),
]


class DownloadError(Exception):
    """Only fixed, credential-free error codes cross this boundary."""


def check_url(url):
    try:
        parsed = urllib.parse.urlsplit(url)
        host = parsed.hostname or ''
        valid = (parsed.scheme == 'https' and parsed.port in (None, 443)
                 and not parsed.username and not parsed.password
                 and any(host == s.lstrip('.') or host.endswith(s) for s in ALLOWED_SUFFIXES))
    except (ValueError, TypeError):
        valid = False
    if not valid:
        raise DownloadError('url_not_allowed')
    return parsed


def detect_extension(data):
    for prefix, name in MAGIC:
        if data.startswith(prefix):
            if name == 'WEBP' and data[8:12] != b'WEBP':
                return None
            return name
    return None


class _CheckedRedirectHandler(urllib.request.HTTPRedirectHandler):
    """Follow redirects only when the target also passes the host allowlist."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        check_url(newurl)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


_opener = urllib.request.build_opener(_CheckedRedirectHandler)


def http_get(url, deadline):
    request = urllib.request.Request(url, headers={'User-Agent': 'StickerNest/1.0 (+local)'})
    try:
        with _opener.open(request, timeout=deadline) as response:
            expected = response.headers.get('Content-Length')
            data = response.read(MAX_BYTES + 1)
        if len(data) > MAX_BYTES:
            raise DownloadError('too_large')
        if expected is not None and len(data) != int(expected):
            raise DownloadError('truncated')
        return data
    except urllib.error.HTTPError as error:
        raise DownloadError(f'http_{error.code}')
    except DownloadError:
        raise
    except Exception:
        raise DownloadError('network')


def load_report(path):
    if not path.exists():
        return {'schema_version': 1, 'expected_items': 0, 'items': []}
    try:
        report = json.loads(path.read_text())
        if (report.get('schema_version') == 1 and isinstance(report.get('items'), list)
                and isinstance(report.get('expected_items'), int)):
            return report
    except (OSError, ValueError):
        pass
    raise DownloadError('bad_report')


def write_report(path, report):
    fd, temp = tempfile.mkstemp(dir=str(path.parent), prefix='.report-', suffix='.tmp')
    try:
        with os.fdopen(fd, 'w') as handle:
            json.dump(report, handle, ensure_ascii=False)
        os.chmod(temp, 0o600)
        os.replace(temp, path)
    except BaseException:
        try:
            os.unlink(temp)
        except OSError:
            pass
        raise


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()

    destination = args.destination
    originals = destination / 'originals'
    if originals.is_symlink():
        raise DownloadError('symlink_originals')
    originals.mkdir(parents=True, exist_ok=True)
    report_path = destination / 'report.json'

    urls = []
    for raw in args.manifest.read_text(errors='replace').splitlines():
        line = raw.strip()
        if line and not line.startswith('#'):
            urls.append(line)
    if not urls:
        raise DownloadError('empty_manifest')
    seen = set()
    deduped = []
    for url in urls:
        if url not in seen:
            seen.add(url)
            deduped.append(url)

    report = load_report(report_path)
    report['expected_items'] = len(deduped)
    report['platform'] = 'wechat'
    # Resume only within the current manifest range; stale rows from a
    # previously longer manifest never leak into the new report.
    done = {item['id']: item for item in report['items']
            if str(item.get('id', '')).isdigit() and int(item['id']) <= len(deduped)}
    counts = {'verified': 0, 'failed': 0, 'skipped': 0}
    for index, url in enumerate(deduped, start=1):
        sticker_id = str(index)
        existing = done.get(sticker_id)
        if existing and existing.get('status') == 'verified' \
                and existing.get('url') == url \
                and (originals / f"{existing['sha256']}.{existing['format']}").exists():
            counts['verified'] += 1
            counts['skipped'] += 1
            report['items'] = [done[key] for key in sorted(done, key=int)]
            write_report(report_path, report)
            continue
        digest = sha256(url.encode('utf-8', 'replace'))
        try:
            check_url(url)
            data = http_get(url, deadline=30)
            extension = detect_extension(data)
            if extension is None:
                raise DownloadError('unknown_format')
            file_hash = sha256(data)
            target = originals / f"{file_hash}.{EXTENSIONS[extension]}"
            if not target.exists():
                temp = None
                try:
                    with tempfile.NamedTemporaryFile(dir=str(originals), prefix='.dl-',
                                                     suffix='.tmp', delete=False) as handle:
                        temp = handle.name
                        handle.write(data)
                        handle.flush()
                        os.fsync(handle.fileno())
                    os.chmod(temp, 0o600)
                    os.link(temp, target)
                    os.unlink(temp)
                    temp = None
                finally:
                    if temp:
                        try:
                            os.unlink(temp)
                        except OSError:
                            pass
            if sha256(target.read_bytes()) != file_hash:
                raise DownloadError('hash_mismatch')
            done[sticker_id] = {
                'id': sticker_id, 'status': 'verified', 'url': url,
                'sha256': file_hash, 'format': EXTENSIONS[extension],
                'resource_identity': digest,
            }
            counts['verified'] += 1
        except DownloadError as error:
            done[sticker_id] = {'id': sticker_id, 'status': 'failed',
                                'resource_identity': digest, 'error': str(error)}
            counts['failed'] += 1
        report['items'] = [done[key] for key in sorted(done, key=int)]
        write_report(report_path, report)
        time.sleep(0.05)

    print(json.dumps({'total': len(deduped), 'verified': counts['verified'],
                      'failed': counts['failed'], 'skipped': counts['skipped']},
                     ensure_ascii=False))
    if counts['failed']:
        failed_ids = [item['id'] for item in report['items']
                      if item.get('status') == 'failed']
        import sys
        print('失败条目:' + '、'.join(failed_ids[:30]), file=sys.stderr)
        raise SystemExit(2)


if __name__ == '__main__':
    try:
        main()
    except DownloadError as error:
        raise SystemExit(f'下载失败:{error}')
