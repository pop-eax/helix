#!/usr/bin/env python3
"""
SNP PSI preprocessing — generate TOML inputs for snp_psi.mpc.

Reads a 23andMe-format SNP file (tab-separated, columns: rsid chromosome
position allele1 allele2) and produces two TOML input files:

  snp_psi_alice.toml  — Person A: first N heterozygous rsids
  snp_psi_bob.toml    — Person B (simulated sibling): N/2 from A + N/2 new
  snp_psi_combined.toml — Alice then Bob concatenated (for single-file benchmarks)

The combined file is what the benchmark loads: alice[0..N] then bob[0..N].

Usage:
  python3 snp_prep.py /path/to/snp_data.txt [N]
  python3 snp_prep.py /path/to/snp_data.txt      # default N=64
"""

import sys
import os
import re
import random

N = int(sys.argv[2]) if len(sys.argv) > 2 else 64
SNP_FILE = sys.argv[1] if len(sys.argv) > 1 else None

if SNP_FILE is None:
    print(__doc__)
    sys.exit(1)

# ── Parse heterozygous variants ───────────────────────────────────────────────

het_rsids = []  # list of (position, rsid_int)

with open(SNP_FILE, encoding="utf-8", errors="replace") as f:
    for line in f:
        line = line.rstrip("\r\n")
        if line.startswith("rsid") or not line.startswith("rs"):
            continue
        parts = line.split("\t")
        if len(parts) < 5:
            continue
        rsid_str, chrom, pos_str, a1, a2 = parts[0], parts[1], parts[2], parts[3], parts[4]
        if a1 == "0" or a2 == "0":
            continue
        if a1 == a2:
            continue  # homozygous — not a carrier variant
        m = re.match(r"rs(\d+)$", rsid_str)
        if not m:
            continue
        rsid_int = int(m.group(1))
        try:
            position = int(pos_str)
        except ValueError:
            continue
        het_rsids.append((position, rsid_int))

# Sort by genomic position for reproducible ordering
het_rsids.sort()
print(f"Found {len(het_rsids)} heterozygous variants")

if len(het_rsids) < 2 * N:
    print(f"ERROR: need at least {2*N} heterozygous variants, found {len(het_rsids)}")
    sys.exit(1)

# ── Build Alice and Bob sets ──────────────────────────────────────────────────
# Alice: first N rsids by genomic position
alice = [rsid for _, rsid in het_rsids[:N]]

# Bob (simulated first-degree relative):
#   - Shares the first N/2 of Alice's variants (IBD segments)
#   - Carries the next N/2 independent variants (from positions N..2N)
shared_count = N // 2
bob_shared   = alice[:shared_count]
bob_private  = [rsid for _, rsid in het_rsids[N:N + (N - shared_count)]]
bob          = bob_shared + bob_private

# Shuffle Bob so Alice can't infer which are shared from ordering
random.seed(42)
random.shuffle(bob)

expected_intersection = shared_count  # by construction

print(f"Alice: {len(alice)} SNPs  |  Bob: {len(bob)} SNPs")
print(f"Expected intersection: {expected_intersection}/{N} "
      f"({100*expected_intersection/N:.0f}% — simulated first-degree relative)")

# ── Write TOML files ──────────────────────────────────────────────────────────

out_dir = os.path.dirname(os.path.abspath(__file__))

def write_toml(path, values, comment):
    with open(path, "w") as f:
        f.write(f"# {comment}\n")
        f.write("[inputs]\n")
        f.write("values = [\n")
        for i, v in enumerate(values):
            sep = "," if i < len(values) - 1 else ""
            f.write(f"  {v}{sep}\n")
        f.write("]\n")
    print(f"Wrote {path}")

write_toml(
    os.path.join(out_dir, "snp_psi_alice.toml"),
    alice,
    f"Alice's {N} heterozygous SNP rsid integers (private genetic data)"
)
write_toml(
    os.path.join(out_dir, "snp_psi_bob.toml"),
    bob,
    f"Bob's {N} heterozygous SNP rsid integers (simulated sibling, {shared_count}/{N} shared)"
)
write_toml(
    os.path.join(out_dir, "snp_psi_combined.toml"),
    alice + bob,
    f"Alice ({N}) then Bob ({N}) — combined input for snp_psi circuit. "
    f"Expected intersection: {expected_intersection}"
)

print(f"\nVerification: actual intersection size = "
      f"{len(set(alice) & set(bob))}")
