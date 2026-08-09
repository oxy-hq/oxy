"""audio-poc CLI.

  python run.py eval-intent            # score the upsell classifier on fixtures
  python run.py detect --source FILE   # run the full pipeline on a file / RTSP

`detect` needs an STT engine (transcribe.py → faster-whisper) and is meant to
run on the edge box against a *consented* recording or the register camera.
`eval-intent` only needs the Anthropic key and runs anywhere.
"""
from __future__ import annotations

import argparse
import sys
from concurrent.futures import ThreadPoolExecutor

import anthropic

import intent
from fixtures import FIXTURES


def eval_intent() -> int:
    client = anthropic.Anthropic()
    rows = list(FIXTURES)

    def run(item):
        text, label, gold_item = item
        v = intent.classify(text, client=client)
        return text, label, gold_item, v

    with ThreadPoolExecutor(max_workers=8) as pool:
        results = list(pool.map(run, rows))

    tp = fp = fn = tn = 0
    item_hits = item_total = 0
    mismatches = []
    for text, label, gold_item, v in results:
        if v.error:
            print(f"  [api-error] {v.error}  «{text[:40]}»")
        if label and v.is_upsell:
            tp += 1
            item_total += 1
            if v.item and gold_item and _item_match(v.item, gold_item):
                item_hits += 1
        elif label and not v.is_upsell:
            fn += 1
            mismatches.append(("MISS  (false neg)", text, gold_item, v))
        elif not label and v.is_upsell:
            fp += 1
            mismatches.append(("FALSE (false pos)", text, v.item, v))
        else:
            tn += 1

    n = len(results)
    prec = tp / (tp + fp) if (tp + fp) else 0.0
    rec = tp / (tp + fn) if (tp + fn) else 0.0
    f1 = 2 * prec * rec / (prec + rec) if (prec + rec) else 0.0
    acc = (tp + tn) / n if n else 0.0

    print("\n==== upsell-intent eval ====")
    print(f"fixtures: {n}   model: {intent.MODEL}")
    print(f"TP={tp}  FP={fp}  FN={fn}  TN={tn}")
    print(f"precision={prec:.2f}  recall={rec:.2f}  f1={f1:.2f}  accuracy={acc:.2f}")
    print(f"item-name correct on true positives: {item_hits}/{item_total}")
    if mismatches:
        print("\n-- mismatches (the interesting failures) --")
        for tag, text, item_, v in mismatches:
            print(f"  {tag}  conf={v.confidence:.2f}  item={item_!r}")
            print(f"        «{text}»")
    else:
        print("\n-- no mismatches --")
    return 0


def _item_match(pred: str, gold: str) -> bool:
    p, g = pred.lower(), gold.lower()
    return p in g or g in p or any(w in p for w in g.split())


def detect(args) -> int:
    try:
        import emit  # noqa: F401 — sink factory (may pull httpx for --emit oxy)
        import pipeline  # STT (faster-whisper) is imported lazily inside run_source
        sink = emit.make_sink(args.emit, only_item=args.only_item)
        return pipeline.run_source(args.source, camera_id=args.camera_id,
                                   window_sec=args.window_sec, sink=sink,
                                   duration_sec=args.max_sec, enhance=args.enhance)
    except ModuleNotFoundError as e:
        print(f"detect needs deps — pip install -r requirements.txt ({e})",
              file=sys.stderr)
        return 2


def main() -> int:
    ap = argparse.ArgumentParser(prog="audio-poc")
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("eval-intent", help="score the classifier on labeled fixtures")
    d = sub.add_parser("detect", help="run the full pipeline on a file/RTSP source")
    d.add_argument("--source", required=True, help="audio file path or rtsp:// URL")
    d.add_argument("--camera-id", default="poc-register", help="register camera id for the event")
    d.add_argument("--window-sec", type=float, default=12.0, help="transcript window length")
    d.add_argument("--emit", choices=["print", "oxy"], default="print",
                   help="print detected events, or POST them to /control/events")
    d.add_argument("--only-item", default=None,
                   help="optional per-item filter (e.g. avocado); unset = emit all upsells (general)")
    d.add_argument("--max-sec", type=float, default=None,
                   help="bound a live RTSP capture to N seconds (e.g. a sample)")
    d.add_argument("--enhance", action="store_true",
                   help="clean up faint/noisy audio before STT (band-limit + denoise + AGC)")
    args = ap.parse_args()
    if args.cmd == "eval-intent":
        return eval_intent()
    if args.cmd == "detect":
        return detect(args)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
