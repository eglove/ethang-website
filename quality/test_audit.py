#!/usr/bin/env python3
"""The audit harness's retry rule, tested without a browser.

`quality/audit.py` re-measures a route whose only failures are marginal,
because three headless browsers sharing the machine move a timing audit by a
few percent. That rule is the difference between a gate that reports real
regressions and one that cries wolf, so it is tested directly: the decision is
a pure function of the first reading, and the phase is driven with a fake
measurement so the retry can be observed rather than inferred.

    python quality/test_audit.py
"""

import sys
import unittest
from pathlib import Path

# Importing `audit` would otherwise write a `__pycache__` entry beside it, which
# is a tracked path in this repository and would show up as a change to the
# harness every time the tests run.
sys.dont_write_bytecode = True

sys.path.insert(0, str(Path(__file__).resolve().parent))

import audit


def problem(score, audit_id='total-blocking-time', category='performance'):
    return (category, audit_id, score, 'Total Blocking Time')


class WorthRetrying(unittest.TestCase):
    def test_a_perfect_route_is_not_retried(self):
        self.assertFalse(audit.worth_retrying([]))

    def test_a_marginal_failure_is_retried(self):
        self.assertTrue(audit.worth_retrying([problem(0.98)]))

    def test_the_threshold_itself_is_retried(self):
        self.assertTrue(audit.worth_retrying([problem(audit.MARGINAL)]))

    def test_a_real_regression_is_not_retried(self):
        # Well below the line: a quieter machine will not fix it, and
        # re-measuring would only delay the report.
        self.assertFalse(audit.worth_retrying([problem(0.60)]))

    def test_one_real_failure_among_marginal_ones_is_not_retried(self):
        self.assertFalse(audit.worth_retrying(
            [problem(0.98), problem(0.40, 'largest-contentful-paint')]))

    def test_an_error_is_not_retried(self):
        # A browser that crashed is not a timing flake; retrying hides it.
        self.assertFalse(
            audit.worth_retrying([problem(0.98)], err='lighthouse exited 1'))


class LighthousePhase(unittest.TestCase):
    """Drive the phase with a fake measurement, so the retry is observable."""

    def setUp(self):
        self.readings = {}
        self.calls = []
        self._real_run = audit.run_lighthouse
        self._real_problems = audit.lighthouse_problems
        audit.run_lighthouse = self._measure
        audit.lighthouse_problems = lambda report: (report['scores'], report['problems'])

    def tearDown(self):
        audit.run_lighthouse = self._real_run
        audit.lighthouse_problems = self._real_problems

    def _measure(self, url, out_path):
        """Answer with the next reading for this route, holding the last."""
        route = url.replace('http://x', '')
        self.calls.append(route)
        queue = self.readings[route]
        seen = self.calls.count(route) - 1
        scores, problems = queue[min(seen, len(queue) - 1)]
        return {'scores': scores, 'problems': problems}, None

    def test_a_marginal_route_is_measured_twice(self):
        self.readings = {'/a/': [({'performance': 0.98}, [problem(0.98)]),
                                 ({'performance': 1.0}, [])]}
        failures, flaky = audit.lighthouse_phase('http://x', ['/a/'], 1)
        self.assertEqual(self.calls.count('/a/'), 2)
        self.assertEqual(failures, [])
        self.assertEqual(len(flaky), 1)
        self.assertIn('/a/', flaky[0])
        self.assertIn('98', flaky[0])

    def test_a_real_failure_is_measured_once(self):
        self.readings = {'/a/': [({'performance': 0.6}, [problem(0.6)])]}
        failures, flaky = audit.lighthouse_phase('http://x', ['/a/'], 1)
        self.assertEqual(self.calls.count('/a/'), 1)
        self.assertEqual(len(failures), 1)
        self.assertEqual(flaky, [])

    def test_a_route_still_marginal_after_the_retry_fails(self):
        self.readings = {'/a/': [({'performance': 0.98}, [problem(0.98)]),
                                 ({'performance': 0.97}, [problem(0.97)])]}
        failures, flaky = audit.lighthouse_phase('http://x', ['/a/'], 1)
        self.assertEqual(self.calls.count('/a/'), 2)
        self.assertEqual(len(failures), 1)
        self.assertEqual(flaky, [])

    def test_no_retry_measures_once_and_fails(self):
        self.readings = {'/a/': [({'performance': 0.98}, [problem(0.98)]),
                                 ({'performance': 1.0}, [])]}
        failures, flaky = audit.lighthouse_phase('http://x', ['/a/'], 1, retry=False)
        self.assertEqual(self.calls.count('/a/'), 1)
        self.assertEqual(len(failures), 1)
        self.assertEqual(flaky, [])

    def test_a_perfect_route_is_measured_once(self):
        self.readings = {'/a/': [({'performance': 1.0}, [])]}
        failures, flaky = audit.lighthouse_phase('http://x', ['/a/'], 1)
        self.assertEqual(self.calls.count('/a/'), 1)
        self.assertEqual(failures, [])
        self.assertEqual(flaky, [])


if __name__ == '__main__':
    unittest.main(verbosity=2)