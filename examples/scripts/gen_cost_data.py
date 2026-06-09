"""Deterministic generator for employees.csv + operating_costs.csv.

Adds two cost layers to the example warehouse so the metric tree has
explicit cost drivers to chase (not just revenue uplift):

- employees.csv     — monthly headcount + salary + benefits per department,
                      ramping from ~80 to ~150 heads over 2024-2025.
- operating_costs.csv — daily OpEx per (category, region). Picks up the
                      same region dimension as the marketing funnel so
                      find_opportunities can compare cost density.

Like gen_funnel_data.py, this is deterministic (fixed seed) so the
fixtures regenerate identically.
"""

import csv
import os
import random
from datetime import date, timedelta

OUT_DIR = "/Users/home/code/oxy-internal/examples/.db"

DEPARTMENTS = [
    # (name, baseline_headcount_2024_01, growth_per_month, avg_salary_2024, benefits_pct)
    ("engineering",       28, 0.030, 14500, 0.28),
    ("sales",             16, 0.025,  9800, 0.22),
    ("marketing",          8, 0.020,  9200, 0.22),
    ("customer_success",  10, 0.022,  7400, 0.20),
    ("operations",        12, 0.015,  6800, 0.18),
    ("g_and_a",            6, 0.010, 12000, 0.25),
]

# (category, baseline_daily_cost, region_specific, growth_per_year, anomaly_window)
# anomaly_window = (start_date, end_date, multiplier) or None
COST_CATEGORIES = [
    ("rent",                       1850, False, 0.04, None),
    ("software_licenses",          1100, False, 0.18, None),
    ("logistics",                  3200, True,  0.06, None),
    ("packaging",                   780, True,  0.05, None),
    ("payment_processing_fees",    1450, False, 0.10, None),
    ("customer_support_tooling",    520, False, 0.12, None),
    ("insurance",                   380, False, 0.05, None),
    # 2025-Q4 logistics spike — region-specific so find_opportunities
    # can surface "latam logistics +60%" instead of just "costs up".
]

REGIONS = [
    # (name, share, region_cost_mult)
    ("us",    0.55, 1.00),
    ("eu",    0.25, 0.92),
    ("apac",  0.12, 0.85),
    ("latam", 0.08, 1.10),  # latam runs hotter on logistics
]

START = date(2024, 1, 1)
END   = date(2025, 12, 31)

random.seed(20260527)


def jitter(base: float, sd_frac: float = 0.06) -> float:
    return max(0.0, random.gauss(base, base * sd_frac))


def months_between(a: date, b: date) -> int:
    return (b.year - a.year) * 12 + (b.month - a.month)


def gen_employees():
    path = os.path.join(OUT_DIR, "employees.csv")
    rows = 0
    with open(path, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow([
            "month", "department", "headcount",
            "avg_salary", "salary_cost", "benefits_cost"
        ])
        d = date(START.year, START.month, 1)
        while d <= END:
            months_in = months_between(START, d)
            for (dept, base_hc, growth, base_salary, benefits_pct) in DEPARTMENTS:
                # Compounded monthly growth, integerised.
                hc_target = base_hc * ((1 + growth) ** months_in)
                headcount = max(1, int(round(jitter(hc_target, 0.03))))
                # Salary creeps ~4% per year.
                salary = jitter(base_salary * (1.04 ** (months_in / 12)), 0.02)
                salary_cost = round(headcount * salary, 2)
                benefits_cost = round(salary_cost * benefits_pct, 2)
                w.writerow([
                    d.isoformat(), dept, headcount,
                    round(salary, 2), salary_cost, benefits_cost
                ])
                rows += 1
            # next month
            if d.month == 12:
                d = date(d.year + 1, 1, 1)
            else:
                d = date(d.year, d.month + 1, 1)
    print(f"wrote {path} — {rows} rows")


def latam_logistics_anomaly(d: date, region: str, category: str) -> float:
    # 2025-Q4 logistics spike in latam (carrier capacity crunch). Real
    # signal for find_opportunities / explain_metric to surface.
    if (
        category == "logistics"
        and region == "latam"
        and d.year == 2025
        and d.month in (10, 11, 12)
    ):
        return 1.65
    return 1.0


def gen_operating_costs():
    path = os.path.join(OUT_DIR, "operating_costs.csv")
    rows = 0
    with open(path, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["cost_date", "category", "region", "amount"])
        d = START
        while d <= END:
            years_in = (d - START).days / 365.0
            for (cat, base, region_specific, yoy, _anom) in COST_CATEGORIES:
                growth_mult = (1 + yoy) ** years_in
                if region_specific:
                    for (region_name, region_share, region_mult) in REGIONS:
                        amount = jitter(
                            base * region_share * growth_mult * region_mult, 0.07
                        )
                        amount *= latam_logistics_anomaly(d, region_name, cat)
                        w.writerow([d.isoformat(), cat, region_name, round(amount, 2)])
                        rows += 1
                else:
                    amount = jitter(base * growth_mult, 0.05)
                    w.writerow([d.isoformat(), cat, "global", round(amount, 2)])
                    rows += 1
            d += timedelta(days=1)
    print(f"wrote {path} — {rows} rows")


if __name__ == "__main__":
    gen_employees()
    gen_operating_costs()
