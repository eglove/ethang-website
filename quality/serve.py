#!/usr/bin/env python3
"""Serve dist/ the way Cloudflare serves it.

wrangler.jsonc declares html_handling "auto-trailing-slash" and
not_found_handling "404-page", and Cloudflare compresses text responses and
sets long cache lifetimes on static assets. This server reproduces all three,
so the audits measure what the deployed site actually serves: a slash-free
request such as /blog/alpha redirects to /blog/alpha/, an unknown path returns
the generated 404.html, and text assets are gzipped when the client accepts
it. Without compression the audits report a first contentful paint that the
real site never pays.

Python's stock handler is single-threaded, which stalls under Lighthouse's
parallel asset fetches, so this uses ThreadingHTTPServer.
"""

import argparse
import functools
import gzip
import http.server
import mimetypes
import os
import posixpath
import re
import sys
import urllib.parse

COMPRESSIBLE = {
    'text/html', 'text/css', 'text/plain', 'text/xml', 'text/xml',
    'application/json', 'application/xml', 'application/javascript',
    'image/svg+xml',
}

EXTRA_TYPES = {
    '.woff2': 'font/woff2',
    '.webp': 'image/webp',
    '.svg': 'image/svg+xml',
    '.ico': 'image/x-icon',
    '.json': 'application/json',
    '.webmanifest': 'application/manifest+json',
}

# Fingerprinted assets never change; HTML must revalidate.
CACHE_ASSET = 'public, max-age=31536000, immutable'
CACHE_HTML = 'public, max-age=0, must-revalidate'


IFRAME_SRC = re.compile(rb'(<iframe\b[^>]*?\bsrc=")([^"]*)(")', re.IGNORECASE)


def neutralize_iframe_src(body):
    """Point every iframe at about:blank, keeping the element and its attributes.

    A page with several third-party embeds never settles: the frames keep
    loading, the load event is delayed, and axe waits on it until it times out.
    Blanking the src keeps the iframe in the DOM, so its title, allow, and
    loading attributes are still audited, while removing the network wait.
    """
    return IFRAME_SRC.sub(rb'\1about:blank\3', body)


class _Server(http.server.ThreadingHTTPServer):
    daemon_threads = True
    request_queue_size = 256


class Handler(http.server.BaseHTTPRequestHandler):
    server_version = 'serve/1.0'

    def __init__(self, request, client_address, server, root=None,
                 neutralize_embeds=False):
        self.root = root
        self.neutralize_embeds = neutralize_embeds
        super().__init__(request, client_address, server)

    def do_GET(self):
        self._respond(send_body=True)

    def do_HEAD(self):
        self._respond(send_body=False)

    def _resolve(self, url_path):
        """Map a URL path onto a file under the served root."""
        had_slash = url_path.endswith('/')
        clean = posixpath.normpath(url_path)
        if clean in ('/', ''):
            return self.root
        parts = [p for p in clean.split('/') if p not in ('', '.', '..')]
        target = os.path.join(self.root, *parts)
        if had_slash and not target.endswith(os.sep):
            target += os.sep
        return target

    def _content_type(self, path):
        ext = os.path.splitext(path)[1].lower()
        if ext in EXTRA_TYPES:
            return EXTRA_TYPES[ext]
        ctype = mimetypes.guess_type(path)[0] or 'application/octet-stream'
        if ctype.startswith('text/') or ctype in ('application/json', 'image/svg+xml'):
            ctype += '; charset=utf-8'
        return ctype

    def _respond(self, send_body):
        path = urllib.parse.unquote(urllib.parse.urlsplit(self.path).path)
        target = self._resolve(path)

        if os.path.isdir(target):
            if not path.endswith('/'):
                # auto-trailing-slash
                self.send_response(301)
                self.send_header('Location', path + '/')
                self.send_header('Content-Length', '0')
                self.end_headers()
                return
            target = os.path.join(target, 'index.html')

        if not os.path.isfile(target):
            self._not_found(send_body)
            return

        with open(target, 'rb') as fh:
            body = fh.read()

        ctype = self._content_type(target)
        if self.neutralize_embeds and ctype.startswith('text/html'):
            body = neutralize_iframe_src(body)
        base_type = ctype.split(';')[0].strip()
        encoding = None
        if (base_type in COMPRESSIBLE and len(body) > 512
                and 'gzip' in self.headers.get('Accept-Encoding', '')):
            body = gzip.compress(body, 6, mtime=0)
            encoding = 'gzip'

        self.send_response(200)
        self.send_header('Content-Type', ctype)
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Vary', 'Accept-Encoding')
        self.send_header('Cache-Control',
                         CACHE_HTML if base_type == 'text/html' else CACHE_ASSET)

        if encoding:
            self.send_header('Content-Encoding', encoding)
        self.end_headers()
        if send_body:
            self.wfile.write(body)

    def _not_found(self, send_body):
        page = os.path.join(self.root, '404.html')
        if not os.path.isfile(page):
            self.send_error(404)
            return
        with open(page, 'rb') as fh:
            body = fh.read()
        self.send_response(404)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        if send_body:
            self.wfile.write(body)

    def log_message(self, fmt, *args):
        if os.environ.get('SERVE_VERBOSE'):
            sys.stderr.write('%s - %s\n' % (self.address_string(), fmt % args))


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--root', default='dist', help='directory to serve (default: dist)')
    ap.add_argument('--port', type=int, default=8080, help='port (0 picks a free one)')
    ap.add_argument('--host', default='127.0.0.1')
    ap.add_argument('--neutralize-embeds', action='store_true',
                    help='point iframe src at about:blank so third-party embeds '
                         'never load (the element itself is kept)')
    args = ap.parse_args()

    root = os.path.abspath(args.root)
    if not os.path.isdir(root):
        sys.exit('serve: no such directory: %s (run: cargo run --release -- build)' % root)

    handler = functools.partial(Handler, root=root,
                               neutralize_embeds=args.neutralize_embeds)
    # The default listen backlog is 5, which refuses connections when the
    # browser opens a burst of parallel asset requests; a refused request shows
    # up in Lighthouse as a console error the site never caused. This has to be
    # set on the class, because `listen()` runs inside the constructor.
    httpd = _Server((args.host, args.port), handler)
    port = httpd.server_address[1]
    # The audit harness waits for this line before running the tools.
    print('LISTENING http://%s:%d' % (args.host, port), flush=True)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        httpd.server_close()


if __name__ == '__main__':
    main()
