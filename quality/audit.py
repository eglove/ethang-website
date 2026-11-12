#!/usr/bin/env python3
"""Audit the built site with Lighthouse and axe-core.

Every generated route is run through both tools. The audit fails unless each
route scores 100 in all four Lighthouse categories and reports no axe
violations under the WCAG 2.0, 2.1, and 2.2 AA rule tags.

    python quality/audit.py                  # build, serve, audit every route
    python quality/audit.py --skip-build     # audit the existing dist/
    python quality/audit.py --only axe       # run one tool
    python quality/audit.py --routes /,/blog # audit specific routes
    python quality/audit.py --jobs 4         # parallel browser runs

Exit code is 0 only when every audited route passes.
"""

import argparse
import json
import shutil
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / 'dist'
REPORTS = ROOT / 'quality' / 'reports'

CATEGORIES = ('performance', 'accessibility', 'best-practices', 'seo')
AXE_TAGS = 'wcag2a,wcag2aa,wcag21a,wcag21aa,wcag22aa'
CHROME_FLAGS = '--headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage'
AXE_CHROME_OPTIONS = 'headless=new,no-sandbox,disable-gpu'


def require(name):
    path = shutil.which(name)
    if not path:
        sys.exit('audit: %s is not on PATH' % name)
    return path


def fmt_score(score):
    return 'n/a' if score is None else '%d' % round(score * 100)


class Server:
    """Serve dist/ on a free port for the lifetime of the audit."""

    def __init__(self, root):
        self.root = root
        self.proc = None
        self.base = None

    def __enter__(self):
        self.proc = subprocess.Popen(
            [sys.executable, str(ROOT / 'quality' / 'serve.py'),
             '--root', str(self.root), '--port', '0', '--neutralize-embeds'],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
        line = self.proc.stdout.readline()
        if not line.startswith('LISTENING '):
            err = self.proc.stderr.read()
            self.proc.kill()
            sys.exit('audit: server did not start: %s%s' % (line.strip(), err.strip()))
        self.base = line.split(' ', 1)[1].strip()
        return self

    def __exit__(self, *exc):
        if self.proc and self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait(timeout=10)
        # Drain the pipes. The server writes nothing after its LISTENING line,
        # but leaving the handles open lets a reader block after the kill and
        # the process outlive the audit, holding dist/ open.
        for stream in (self.proc.stdout, self.proc.stderr):
            if stream:
                try:
                    stream.close()
                except Exception:
                    pass


def discover_routes(root):
    """Every directory holding an index.html is a route, matching the deploy.

    Routes are returned in the trailing-slash form the site serves them at.
    `wrangler.jsonc` sets `html_handling: "auto-trailing-slash"`, so a
    slash-free request is answered with a 301 - auditing that form would charge
    every page for a redirect the canonical URL never takes.
    """
    routes = []
    for index in root.rglob('index.html'):
        rel = index.parent.relative_to(root).as_posix()
        routes.append('/' if rel == '.' else '/' + rel + '/')
    return sorted(routes)


def build():
    print('building dist/ ...', flush=True)
    proc = subprocess.run(['cargo', 'run', '--release', '--', 'build'], cwd=ROOT)
    if proc.returncode != 0:
        sys.exit('audit: build failed')


def run_lighthouse(url, out_path):
    out_path.parent.mkdir(parents=True, exist_ok=True)
    cmd = [require('lighthouse'), url,
           '--quiet',
           '--output=json',
           '--output-path=' + str(out_path),
           '--only-categories=' + ','.join(CATEGORIES),
           '--chrome-flags=' + CHROME_FLAGS]
    proc = subprocess.run(cmd, capture_output=True, text=True, timeout=900)
    if proc.returncode != 0 or not out_path.exists():
        detail = (proc.stderr or proc.stdout or '').strip()
        return None, 'lighthouse exited %d: %s' % (proc.returncode, detail[-1500:])
    return json.loads(out_path.read_text(encoding='utf-8')), None


def lighthouse_problems(report):
    """Return the category scores and every weighted audit below 1.0."""
    audits = report.get('audits', {})
    scores = {}
    failures = []
    for name, cat in report.get('categories', {}).items():
        scores[name] = cat.get('score')
        for ref in cat.get('auditRefs', []):
            if not ref.get('weight'):
                continue
            audit = audits.get(ref['id'], {})
            score = audit.get('score')
            if score is not None and score < 1:
                failures.append((name, ref['id'], score, audit.get('title', '')))
    return scores, failures


def run_axe(url, driver, timeout):
    """Run axe, reading results from the file it writes.

    axe's --stdout is unreliable when several runs share a console, so each
    run gets its own output file and the file is read back.
    """
    out_dir = REPORTS / 'axe'
    out_dir.mkdir(parents=True, exist_ok=True)
    name = 'axe' + url.split('://', 1)[1].split('/', 1)[-1].replace('/', '_') + '.json'
    out = out_dir / name
    if out.exists():
        out.unlink()
    cmd = [require('axe'), url,
           '--tags', AXE_TAGS,
           '--dir', str(out_dir),
           '--save', name,
           '--chromedriver-path', driver,
           '--chrome-options', AXE_CHROME_OPTIONS,
           '--timeout', str(timeout)]
    proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout + 180)
    if proc.returncode != 0:
        detail = (proc.stderr or proc.stdout or '').strip()
        return None, 'axe exited %d: %s' % (proc.returncode, detail[-1500:])
    if not out.exists():
        detail = (proc.stderr or proc.stdout or '').strip()
        return None, 'axe wrote no report: %s' % detail[-1500:]
    try:
        data = json.loads(out.read_text(encoding='utf-8'))
    except json.JSONDecodeError as exc:
        return None, 'axe produced unreadable output: %s' % exc
    if isinstance(data, dict):
        data = [data]
    return data, None


def lighthouse_phase(base, routes, jobs):
    failures = []

    def one(route):
        url = base + route
        out = REPORTS / ('lighthouse' + route.replace('/', '_') + '.json')
        report, err = run_lighthouse(url, out)
        if err:
            return route, None, [], err
        scores, problems = lighthouse_problems(report)
        return route, scores, problems, None

    with ThreadPoolExecutor(max_workers=jobs) as pool:
        for route, scores, problems, err in pool.map(one, routes):
            if err:
                print('  %-58s ERROR' % route, flush=True)
                failures.append('%s: %s' % (route, err))
                continue
            line = '  %-58s ' % route
            line += ' '.join('%s=%s' % (k, fmt_score(v)) for k, v in scores.items())
            print(line, flush=True)
            for cat, audit, score, title in problems:
                failures.append('%s [%s] %s scored %s (%s)'
                                % (route, cat, audit, fmt_score(score), title))
    return failures


def axe_phase(base, routes, driver, jobs, strict):
    failures = []
    warnings = []

    def one(route):
        data, err = run_axe(base + route, driver, 120)
        return route, data, err

    with ThreadPoolExecutor(max_workers=jobs) as pool:
        for route, data, err in pool.map(one, routes):
            if err:
                print('  %-58s ERROR' % route, flush=True)
                failures.append('%s: %s' % (route, err))
                continue
            violations = []
            incomplete = []
            for result in data:
                violations.extend(result.get('violations', []))
                incomplete.extend(result.get('incomplete', []))
            status = 'pass' if not violations else 'FAIL (%d)' % len(violations)
            print('  %-58s %s' % (route, status), flush=True)
            for v in violations:
                failures.append('%s: %s (%s, %d node(s))'
                                % (route, v.get('id'), v.get('impact'),
                                   len(v.get('nodes', []))))
            for i in incomplete:
                warnings.append('%s: %s (%s)' % (route, i.get('id'), i.get('impact')))
                if strict:
                    failures.append('%s: %s could not be determined'
                                    % (route, i.get('id')))
    return failures, warnings


def main():
    ap = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--skip-build', action='store_true',
                    help='audit the existing dist/ instead of rebuilding')
    ap.add_argument('--only', choices=('lighthouse', 'axe'),
                    help='run a single tool')
    ap.add_argument('--routes', help='comma separated routes (default: all)')
    ap.add_argument('--jobs', type=int, default=3,
                    help='parallel Lighthouse runs (default: 3)')
    ap.add_argument('--strict-incomplete', action='store_true',
                    help='also fail when axe cannot determine a result')
    args = ap.parse_args()

    if not args.skip_build:
        build()
    if not DIST.is_dir():
        sys.exit('audit: no dist/ - run: cargo run --release -- build')

    routes = discover_routes(DIST)
    if args.routes:
        wanted = [r.strip() for r in args.routes.split(',') if r.strip()]
        unknown = [r for r in wanted if r not in routes]
        if unknown:
            sys.exit('audit: unknown route(s): %s' % ', '.join(unknown))
        routes = wanted
    if not routes:
        sys.exit('audit: no routes to audit')

    driver = require('chromedriver')
    REPORTS.mkdir(parents=True, exist_ok=True)
    started = time.time()

    with Server(DIST) as server:
        print('serving %s at %s' % (DIST, server.base), flush=True)
        print('auditing %d routes, %d parallel runs' % (len(routes), args.jobs),
              flush=True)

        failures = []
        warnings = []

        if args.only in (None, 'lighthouse'):
            print('', flush=True)
            print('--- lighthouse ---', flush=True)
            failures += lighthouse_phase(server.base, routes, args.jobs)
        if args.only in (None, 'axe'):
            print('', flush=True)
            print('--- axe-core (WCAG 2.0/2.1/2.2 AA) ---', flush=True)
            # axe drives one chromedriver per run; concurrent instances
            # deadlock, so this phase is sequential.
            f, w = axe_phase(server.base, routes, driver, 1,
                             args.strict_incomplete)
            failures += f
            warnings += w

    elapsed = time.time() - started
    print('')
    if warnings:
        print('axe could not determine a result for %d finding(s):' % len(warnings))
        for w in warnings:
            print('  ! ' + w)
        print('  (pass --strict-incomplete to fail on these)')
        print('')
    if failures:
        print('FAILED: %d problem(s) in %.1fs' % (len(failures), elapsed))
        for f in failures:
            print('  x ' + f)
        return 1
    print('PASS: %d routes, perfect Lighthouse scores and no axe violations (%.1fs)'
          % (len(routes), elapsed))
    return 0


if __name__ == '__main__':
    sys.exit(main())
