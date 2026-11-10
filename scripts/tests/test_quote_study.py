import copy
from datetime import date
import unittest
from unittest.mock import Mock
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import build_quote_study as study


def bar(t, price):
    return {"end_period_ts": t, "yes_ask": {k + "_dollars": price for k in ("open", "low", "high", "close")}}


class StudyTests(unittest.TestCase):
    def test_exact_cutoff_only_interior_prices_and_unique_minutes(self):
        data = [bar(240, ".33"), bar(300, ".1999"), bar(540, ".31"), bar(840, ".005"), bar(840, ".005")]
        self.assertEqual(study.cutoff_asks(data, 0), {"5": 1999})
        for value in (None, "0", "1", "invalid", "NaN", ".00001", "1.01"):
            self.assertEqual(study.cutoff_asks([bar(300, value)], 0), {})
        broken = bar(300, ".4")
        broken["yes_ask"]["high_dollars"] = ".3"
        self.assertEqual(study.cutoff_asks([broken], 0), {})

    def test_features_are_independent_of_labels_and_later_bars(self):
        day = date(2026, 11, 9)
        start = int(study.midnight(day).timestamp())
        ticker = "KXBTC15M-TEST"
        market = {"ticker": ticker, "open_time": "2026-11-09T00:00:00Z", "close_time": "2026-11-09T00:15:00Z", "result": "yes"}
        summary = {"asset": "BTC", "ticker": ticker, "captured_at": "2026-11-09T00:25:00Z", "result": "yes"}
        payload = {"schema_version": 1, "market": market, "candlesticks": {"ticker": ticker, "candlesticks": [bar(start + 300, ".2000")]}}
        def doc():
            return {"name": study.ROOT + "/contracts_btc/" + ticker, "updateTime": summary["captured_at"],
                    "fields": {"payload": {"stringValue": study.encode(payload)}, "summary": {"stringValue": study.encode(summary)},
                               "close_time": {"timestampValue": market["close_time"]}}}
        a = study.partition("BTC", day, [doc()])
        market["result"] = summary["result"] = "no"
        payload["candlesticks"]["candlesticks"].append(bar(start + 900, ".99"))
        b = study.partition("BTC", day, [doc()])
        self.assertEqual(a["features"], b["features"])
        self.assertEqual(study.cohort([a], 5)["bins"][1], {"n": 1, "yes": 1, "sum_ask_bps": 2000})
        self.assertEqual(study.cohort([b], 5)["bins"][1]["yes"], 0)
        self.assertEqual(study.cohort([a], 10)["expected"], 96)
        self.assertEqual(sum(x["n"] for x in study.cohort([a], 10)["bins"]), 0)
        with self.assertRaises(ValueError):
            study.partition("ETH", day, [doc()])
        with self.assertRaises(ValueError):
            study.partition("BTC", day, [doc(), doc()])

    def test_atomic_writes_retry_and_older_snapshot(self):
        part = {"day": "2026-11-09", "asset": "BTC", "features": {}, "labels": {}}
        board = {"end_day": part["day"]}
        at = "2026-11-10T05:00:00Z"
        docs, db = {}, Mock()
        db.get.side_effect = docs.get
        def commit(path, req):
            self.assertEqual(path, ":commit")
            for w in req["writes"]:
                name = w["update"]["name"].split("/")[-1]
                self.assertEqual(w["currentDocument"], {"updateTime": at} if name in docs else {"exists": False})
                docs[name] = dict(w["update"], updateTime=at)
            return {"writeResults": [{}] * len(req["writes"])}
        db.request.side_effect = commit
        self.assertEqual(study.publish(db, [part], board, None, at), 2)
        self.assertEqual(study.publish(db, [part], board, docs["latest"], at), 0)
        with self.assertRaises(ValueError):
            study.publish(db, [part], board, docs["latest"], "2026-11-09T03:00:00Z")
        revised = copy.deepcopy(part)
        revised["labels"]["T"] = "no"
        self.assertEqual(study.publish(db, [revised], board, docs["latest"], at), 2)
        self.assertEqual(len(docs), 3)
