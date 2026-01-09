#!/usr/bin/env python3

# This script parses two JSON files containing evaluation metrics from Psyche and lm_eval,
# and compares the metrics to check if they are within a 10% margin of each other.

import json
import sys


# Returns (is_in_margin, margin_difference)
def compare_within_margin(psyche, lm_eval, margin=10):
    try:
        p = float(psyche)
        l = float(lm_eval)

        if l == 0:
            return (p == 0, 0.0)

        diff_percent = abs((p - l) / l * 100)
        within_margin = diff_percent <= margin
        return (within_margin, diff_percent)
    except (ValueError, ZeroDivisionError):
        return (False, 0.0)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: compare_metrics.py <psyche_results.json> <lm_eval_results.json>")
        sys.exit(1)

    psyche_file = sys.argv[1]
    lm_eval_file = sys.argv[2]

    try:
        with open(psyche_file, "r") as f:
            psyche_data = json.load(f)
        with open(lm_eval_file, "r") as f:
            lm_eval_data = json.load(f)

        psyche_acc = psyche_data["metrics"]["acc"]
        lm_eval_acc = lm_eval_data["metrics"]["acc"]
        psyche_acc_norm = psyche_data["metrics"]["acc_norm"]
        lm_eval_acc_norm = lm_eval_data["metrics"]["acc_norm"]

        # Compare acc
        acc_match, acc_diff = compare_within_margin(psyche_acc, lm_eval_acc)
        status_acc = "✅ PASS" if acc_match else f"❌ FAIL ({acc_diff:.1f}% diff)"
        print(
            f"acc:      Psyche={psyche_acc:.4f}  lm_eval={lm_eval_acc:.4f} - {status_acc}"
        )

        # Compare acc_norm
        norm_match, norm_diff = compare_within_margin(psyche_acc_norm, lm_eval_acc_norm)
        status_norm = "✅ PASS" if norm_match else f"❌ FAIL ({norm_diff:.1f}% diff)"
        print(
            f"acc_norm: Psyche={psyche_acc_norm:.4f}  lm_eval={lm_eval_acc_norm:.4f} - {status_norm}"
        )

    except FileNotFoundError as e:
        print(f"Error: Could not find file {e}")
        sys.exit(1)
    except KeyError as e:
        print(f"Error: Missing metric in JSON {e}")
        sys.exit(1)
    except json.JSONDecodeError as e:
        print(f"Error: Invalid JSON format {e}")
        sys.exit(1)
