"""Deterministic generator for marketing_funnel.csv + marketing_spend.csv.

Designed to give the metric tree something meaty to chew on:
- daily × channel × device grain → low-card dims that find_opportunities can rank
- realistic conversion-rate variation: paid_search is the best channel,
  email is the worst, so explain_metric / find_opportunities have a real signal
- a deliberate 2025-09 dip in paid_search signup_rate so explain_metric
  has a concentrated period-over-period move to decompose
- spend correlates with sessions (CAC-style driver edge) but with channel
  efficiency differences, so sensitivity has structure to walk
"""

import csv
import os
import random
from datetime import date, timedelta

OUT_DIR = "/Users/home/code/oxy-internal/examples/.db"

CHANNELS = [
    # (name, baseline_sessions_per_day, lead_rate, signup_rate, paid_conv_rate, spend_per_session, ctr)
    ("organic",      4200, 0.085, 0.42, 0.18, 0.0,  0.038),
    ("paid_search",  3100, 0.110, 0.55, 0.28, 1.85, 0.062),
    ("paid_social",  2700, 0.070, 0.38, 0.14, 1.40, 0.024),
    ("email",         900, 0.140, 0.31, 0.09, 0.05, 0.110),
    ("referral",     1500, 0.095, 0.48, 0.22, 0.0,  0.045),
]

DEVICES = [
    # (name, share, signup_multiplier)
    ("desktop", 0.45, 1.10),
    ("mobile",  0.45, 0.85),
    ("tablet",  0.10, 1.00),
]

REGIONS = [
    # (name, share, paid_conv_multiplier)
    ("us",    0.55, 1.05),
    ("eu",    0.25, 0.95),
    ("apac",  0.12, 1.15),  # apac converts well on paid -> opportunity gap is small
    ("latam", 0.08, 0.70),  # latam underperforms -> clear opportunity
]

START = date(2024, 1, 1)
END   = date(2025, 12, 31)

random.seed(20260527)

def jitter(base: float, sd_frac: float = 0.08) -> float:
    return max(0.0, random.gauss(base, base * sd_frac))

def day_of_week_factor(d: date) -> float:
    # B2B-ish pattern: weekdays slightly higher
    return 1.0 if d.weekday() < 5 else 0.85

def seasonal_factor(d: date) -> float:
    # Slow Q1 ramp, Q4 holiday lift
    month = d.month
    if month in (1, 2): return 0.92
    if month in (11, 12): return 1.18
    return 1.0

def paid_search_anomaly_factor(d: date, channel: str) -> float:
    # 2025-09: paid_search signup_rate drops 35% — a real signal for
    # explain_metric / find_opportunities to surface.
    if channel == "paid_search" and d.year == 2025 and d.month == 9:
        return 0.65
    return 1.0


def gen_funnel():
    path = os.path.join(OUT_DIR, "marketing_funnel.csv")
    rows = 0
    with open(path, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow([
            "event_date", "channel", "region", "device",
            "sessions", "leads", "signups", "paid_signups"
        ])
        d = START
        while d <= END:
            dow = day_of_week_factor(d)
            seas = seasonal_factor(d)
            for ch_name, base_sessions, lead_rate, signup_rate, paid_conv, _, _ in CHANNELS:
                for dev_name, dev_share, dev_signup_mult in DEVICES:
                    for region_name, region_share, region_paid_mult in REGIONS:
                        # sessions: baseline × DoW × seasonal × dev × region × jitter
                        sess_base = base_sessions * dev_share * region_share * dow * seas
                        sessions = int(round(jitter(sess_base, 0.10)))
                        if sessions <= 0:
                            continue
                        lr = max(0.001, jitter(lead_rate, 0.06))
                        leads = int(round(sessions * lr))
                        sr = max(0.001, jitter(signup_rate * dev_signup_mult, 0.07))
                        signups = int(round(leads * sr))
                        pc = max(0.001, jitter(paid_conv * region_paid_mult, 0.07))
                        pc *= paid_search_anomaly_factor(d, ch_name)
                        paid_signups = int(round(signups * pc))
                        w.writerow([
                            d.isoformat(), ch_name, region_name, dev_name,
                            sessions, leads, signups, paid_signups
                        ])
                        rows += 1
            d += timedelta(days=1)
    print(f"wrote {path} — {rows} rows")


def gen_spend():
    path = os.path.join(OUT_DIR, "marketing_spend.csv")
    rows = 0
    with open(path, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["event_date", "channel", "spend", "clicks", "impressions"])
        d = START
        while d <= END:
            dow = day_of_week_factor(d)
            seas = seasonal_factor(d)
            for ch_name, base_sessions, _, _, _, spend_per_sess, ctr in CHANNELS:
                # Sessions at the channel-level (sum across devices/regions)
                sess_total = base_sessions * dow * seas
                sessions = int(round(jitter(sess_total, 0.10)))
                spend = round(sessions * spend_per_sess * jitter(1.0, 0.05), 2)
                # Clicks ~= sessions for paid; lower for non-paid; spend=0 channels have no clicks
                if spend_per_sess == 0.0:
                    clicks = 0
                    impressions = 0
                else:
                    clicks = sessions
                    actual_ctr = max(0.001, jitter(ctr, 0.05))
                    impressions = int(round(clicks / actual_ctr))
                w.writerow([d.isoformat(), ch_name, spend, clicks, impressions])
                rows += 1
            d += timedelta(days=1)
    print(f"wrote {path} — {rows} rows")


if __name__ == "__main__":
    gen_funnel()
    gen_spend()
