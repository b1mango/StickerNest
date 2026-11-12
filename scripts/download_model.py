#!/usr/bin/env python3
"""Download and verify the classifier model and character pack."""
import argparse
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request

MODEL = 'laion-clip-vit-b32'
FILES = {'vision.safetensors', 'characters.json'}
READ_TIMEOUT = 30
ATTEMPTS = 4


class DownloadError(Exception):
    pass


# GitHub release downloads redirect to release-assets (current) or objects (legacy).
ALLOWED_HOSTS = ('github.com', 'objects.githubusercontent.com', 'release-assets.githubusercontent.com')


def check_url(url):
    try:
        parsed = urllib.parse.urlsplit(url)
        valid = (isinstance(url, str) and parsed.scheme == 'https' and parsed.netloc in ALLOWED_HOSTS
                 and url.startswith(tuple(f'https://{host}/' for host in ALLOWED_HOSTS)))
    except (TypeError, ValueError):
        valid = False
    if not valid:
        raise DownloadError('下载地址不在允许的 GitHub 域名内')
    return url


def validate_manifest(manifest):
    if (not isinstance(manifest, dict) or type(manifest.get('version')) is not int
            or manifest.get('version') != 1 or manifest.get('model') != MODEL):
        raise DownloadError('模型清单版本或型号无效')
    files = manifest.get('files')
    if not isinstance(files, list) or len(files) != len(FILES):
        raise DownloadError('模型清单文件列表无效')
    names = set()
    for item in files:
        if (not isinstance(item, dict) or not isinstance(item.get('name'), str)
                or item['name'] not in FILES or item['name'] in names):
            raise DownloadError('模型清单文件名无效')
        names.add(item['name'])
        check_url(item.get('url'))
        if (not isinstance(item.get('size'), int) or isinstance(item['size'], bool)
                or item['size'] <= 0 or not isinstance(item.get('sha256'), str)
                or not re.fullmatch(r'[a-fA-F0-9]{64}', item['sha256'])):
            raise DownloadError('模型清单文件大小或校验值无效')
    return files


class GitHubRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        check_url(newurl)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def open_url(opener, url, headers=None):
    check_url(url)
    request = urllib.request.Request(url, headers=headers or {})
    response = opener.open(request, timeout=READ_TIMEOUT)
    check_url(response.geturl())
    return response


def retry(operation):
    for attempt in range(ATTEMPTS):
        try:
            return operation()
        except DownloadError:
            raise
        except (OSError, ValueError, http.client.HTTPException):
            if attempt == ATTEMPTS - 1:
                raise DownloadError('网络或本地文件操作失败，已重试 3 次') from None


def digest(path):
    sha = hashlib.sha256()
    with path.open('rb') as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b''):
            sha.update(chunk)
    return sha.hexdigest()


def atomic_write(path, data):
    fd, temp = tempfile.mkstemp(prefix='.manifest-', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as file:
            file.write(data)
            file.flush()
            os.fsync(file.fileno())
        os.replace(temp, path)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


def proxies():
    """Environment proxies when set, otherwise the macOS system proxy: GitHub
    usually needs one in China, and a stray NO_PROXY alone must not disable it."""
    found = urllib.request.getproxies_environment()
    if not (found.get('https') or found.get('http')):
        try:
            found = {**urllib.request.getproxies_macosx_sysconf(), **found}
        except (AttributeError, OSError):
            pass
    return found


def download(manifest_url, destination, opener=None):
    opener = opener or urllib.request.build_opener(urllib.request.ProxyHandler(proxies()), GitHubRedirect())
    def get_manifest():
        with open_url(opener, manifest_url) as response:
            return response.read()
    manifest_bytes = retry(get_manifest)
    try:
        files = validate_manifest(json.loads(manifest_bytes))
    except (ValueError, UnicodeDecodeError) as exc:
        raise DownloadError('模型清单不是有效 JSON') from exc
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    total = sum(item['size'] for item in files)
    completed = 0
    last_percent = -1

    def progress(done):
        nonlocal last_percent
        percent = done * 100 // total
        if percent > last_percent:
            print('PROGRESS ' + json.dumps({'stage': 'downloading-model', 'message': '正在下载形象识别模型',
                                           'done': done, 'total': total}, ensure_ascii=False),
                  file=sys.stderr, flush=True)
            last_percent = percent

    for item in files:
        target = destination / item['name']
        part = destination / (item['name'] + '.part')
        size, expected = item['size'], item['sha256'].lower()
        if target.is_file() and target.stat().st_size == size and digest(target) == expected:
            completed += size
            progress(completed)
            continue

        def transfer():
            offset = part.stat().st_size if part.exists() else 0
            if offset > size:
                part.unlink()
                offset = 0
            progress(completed + offset)
            if offset < size:
                headers = {'Range': f'bytes={offset}-'} if offset else {}
                with open_url(opener, item['url'], headers) as response:
                    status = response.getcode()
                    if offset and status == 206:
                        content_range = response.headers.get('Content-Range', '')
                        if not re.fullmatch(rf'bytes {offset}-{size - 1}/{size}', content_range):
                            raise DownloadError('服务器返回的断点续传范围无效')
                        mode = 'ab'
                    elif status == 200:
                        offset, mode = 0, 'wb'
                    else:
                        raise DownloadError('服务器返回的下载响应无效')
                    with part.open(mode) as file:
                        while True:
                            chunk = response.read(256 * 1024)
                            if not chunk:
                                break
                            offset += len(chunk)
                            if offset > size:
                                raise DownloadError('下载文件大小超出清单')
                            file.write(chunk)
                            progress(completed + offset)
                        file.flush()
                        os.fsync(file.fileno())
            if part.stat().st_size != size:
                raise OSError('incomplete download')
            if digest(part) != expected:
                part.unlink()
                raise DownloadError('下载文件的 SHA-256 校验不一致')
            os.replace(part, target)

        retry(transfer)
        completed += size
        progress(completed)
    atomic_write(destination / 'manifest.json', manifest_bytes)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest_url')
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    try:
        download(args.manifest_url, args.destination)
    except (DownloadError, OSError) as exc:
        print(f'形象识别下载失败：{exc}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
