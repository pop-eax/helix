#!/usr/bin/env python3
"""
Generate threat model diagrams for the Helix BGW protocol.

  threat_model_overview.png  — Parties, channels, adversary capabilities, proved properties
  threat_model_phases.png    — Sequence-style per-phase data flows
"""

import subprocess, pathlib

OUT = pathlib.Path(__file__).parent

# ─────────────────────────────────────────────────────────────────────────────
# Diagram 1 — Overview: parties, trust zones, adversary, proved properties
# ─────────────────────────────────────────────────────────────────────────────

overview = r"""
digraph Helix_BGW_Threat_Model {
  graph [
    label     = "Helix BGW — Threat Model\n(n=3, t=2 threshold, passive adversary, authenticated channels)"
    labelloc  = "t"
    fontsize  = "15"
    fontname  = "Helvetica"
    bgcolor   = "#f8f9fa"
    pad       = "0.6"
    splines   = "curved"
    rankdir   = "LR"
    nodesep   = "0.7"
    ranksep   = "1.5"
  ]
  node [fontname="Helvetica" fontsize="11" style="filled" penwidth="1.5"]
  edge [fontname="Helvetica" fontsize="9"]

  // ── Dealer ────────────────────────────────────────────────────────────────
  D [label="Dealer (D)\nBeaverSource" shape="diamond"
     fillcolor="#d4edda" color="#155724" width="1.6" height="1.0"]

  // ── Computing parties ─────────────────────────────────────────────────────
  subgraph cluster_parties {
    label     = "Computing Parties   (Honest Execution Zone)"
    style     = "dashed"
    color     = "#28a745"
    bgcolor   = "#f0fff4"
    fontsize  = "11"
    fontcolor = "#155724"

    P1 [label="Party P₁\nprivate input x₁" shape="box"
        fillcolor="#cce5ff" color="#004085"]
    P2 [label="Party P₂\nprivate input x₂" shape="box"
        fillcolor="#cce5ff" color="#004085"]
    P3 [label="Party P₃\nprivate input x₃" shape="box"
        fillcolor="#cce5ff" color="#004085"]
  }

  // ── Adversary ─────────────────────────────────────────────────────────────
  subgraph cluster_adv {
    label     = "Adversary  (passive, up to t−1 = 1 corruptions)"
    style     = "dashed"
    color     = "#dc3545"
    bgcolor   = "#fff5f5"
    fontsize  = "11"
    fontcolor = "#721c24"

    ADV [label="Adversary  𝒜\n(Dolev-Yao + corruption)" shape="octagon"
         fillcolor="#f8d7da" color="#721c24" width="2.0"]
  }

  // ── Authenticated channels (pairwise, pre-shared keys) ────────────────────
  // Dealer → parties
  D  -> P1 [label="k_D1  (AuthChan)" color="#555" style="dashed" dir="both"]
  D  -> P2 [label="k_D2  (AuthChan)" color="#555" style="dashed" dir="both"]
  D  -> P3 [label="k_D3  (AuthChan)" color="#555" style="dashed" dir="both"]

  // Party ↔ party
  P1 -> P2 [label="k_12  (AuthChan)" color="#555" style="dashed" dir="both"]
  P1 -> P3 [label="k_13  (AuthChan)" color="#555" style="dashed" dir="both"]
  P2 -> P3 [label="k_23  (AuthChan)" color="#555" style="dashed" dir="both"]

  // ── Corruption edges ──────────────────────────────────────────────────────
  ADV -> P1 [label="Corrupt(P₁)\n→ leaks k_P₁→*\n→ decrypts P₁ sends\n  but mk_share opaque"
             color="#dc3545" fontcolor="#dc3545" style="dotted"
             penwidth="2" arrowhead="odiamond"]
  ADV -> D  [label="Corrupt('D')\n→ leaks k_D→*\n→ decrypts triple shares\n  breaks triple_secrecy"
             color="#dc3545" fontcolor="#dc3545" style="dotted"
             penwidth="2" arrowhead="odiamond"]

  // ── Proved security properties (annotation node) ──────────────────────────
  PROPS [shape="none" label=<
    <TABLE BORDER="1" CELLBORDER="0" CELLSPACING="4" CELLPADDING="5"
           BGCOLOR="#ffffff" COLOR="#343a40">
      <TR><TD COLSPAN="2" BGCOLOR="#343a40">
        <FONT COLOR="white"><B>Tamarin-Verified Security Properties</B></FONT>
      </TD></TR>
      <TR><TD BGCOLOR="#e7f3ff" WIDTH="14"> </TD>
          <TD ALIGN="LEFT"><B>Phase 1 – Input Sharing</B><BR/>
          ✓ share_secrecy (ZT-1, ZT-3)<BR/>
          ✓ share_authentication (ZT-2)<BR/>
          ✓ share_exchange_reachable</TD></TR>
      <TR><TD BGCOLOR="#d4edda" WIDTH="14"> </TD>
          <TD ALIGN="LEFT"><B>Phase 2 – Beaver Triple Distrib.</B><BR/>
          ✓ triple_secrecy (ZT-1, ZT-3)<BR/>
          ✓ triple_share_authenticity (ZT-2)<BR/>
          ✓ triple_distribution_reachable</TD></TR>
      <TR><TD BGCOLOR="#fff3e0" WIDTH="14"> </TD>
          <TD ALIGN="LEFT"><B>Phase 3 – Multiplication (δ/ε)</B><BR/>
          ✓ mul_broadcast_authentication (ZT-2)<BR/>
          ✓ multiplication_reachable</TD></TR>
      <TR><TD BGCOLOR="#ede7f6" WIDTH="14"> </TD>
          <TD ALIGN="LEFT"><B>Phase 4 – Output Reconstruction</B><BR/>
          ✓ output_share_authentication (ZT-2)<BR/>
          ✓ output_reconstruction_reachable</TD></TR>
      <TR><TD BGCOLOR="#f8d7da" WIDTH="14"> </TD>
          <TD ALIGN="LEFT"><B>Adversary model</B><BR/>
          Passive (semi-honest); ≤ t−1 corruptions<BR/>
          No message forgery under authenticated channels<BR/>
          Type tags prevent cross-phase replay</TD></TR>
    </TABLE>
  >]

  ADV -> PROPS [style="invis"]
}
"""

# ─────────────────────────────────────────────────────────────────────────────
# Diagram 2 — Phase sequence with data flows
# ─────────────────────────────────────────────────────────────────────────────

phases = r"""
digraph Helix_BGW_Phases {
  graph [
    label     = "Helix BGW — Protocol Phases & Data Flows\n(Tamarin-verified: all 10 lemmas hold)"
    labelloc  = "t"
    fontsize  = "15"
    fontname  = "Helvetica"
    bgcolor   = "#f8f9fa"
    pad       = "0.5"
    splines   = "ortho"
    rankdir   = "TB"
    nodesep   = "1.0"
    ranksep   = "0.6"
  ]
  node [fontname="Helvetica" fontsize="10" style="filled" penwidth="1.2"]
  edge [fontname="Helvetica" fontsize="8"]

  // ── Column headers ────────────────────────────────────────────────────────
  HD  [label="Dealer (D)"    fillcolor="#d4edda" color="#155724" shape="box" width="2.2"]
  HP1 [label="Party P₁"     fillcolor="#cce5ff" color="#004085" shape="box" width="2.2"]
  HP2 [label="Party P₂"     fillcolor="#cce5ff" color="#004085" shape="box" width="2.2"]
  HP3 [label="Party P₃"     fillcolor="#cce5ff" color="#004085" shape="box" width="2.2"]
  HADV[label="Adversary 𝒜"  fillcolor="#f8d7da" color="#721c24" shape="box" width="2.2"]
  { rank=same; HD; HP1; HP2; HP3; HADV }

  // ── Spacer nodes (invisible) to enforce column positions ──────────────────
  node [shape="point" width="0" style="invis"]
  SD1 SP11 SP21 SP31 SA1
  SD2 SP12 SP22 SP32 SA2
  SD3 SP13 SP23 SP33 SA3
  SD4 SP14 SP24 SP34 SA4

  { rank=same; SD1; SP11; SP21; SP31; SA1 }
  { rank=same; SD2; SP12; SP22; SP32; SA2 }
  { rank=same; SD3; SP13; SP23; SP33; SA3 }
  { rank=same; SD4; SP14; SP24; SP34; SA4 }

  HD   -> SD1 -> SD2 -> SD3 -> SD4 [style="invis"]
  HP1  -> SP11-> SP12-> SP13-> SP14 [style="invis"]
  HP2  -> SP21-> SP22-> SP23-> SP24 [style="invis"]
  HP3  -> SP31-> SP32-> SP33-> SP34 [style="invis"]
  HADV -> SA1 -> SA2 -> SA3 -> SA4  [style="invis"]

  node [shape="box" style="filled" penwidth="1.2"]

  // ════════════════════════════════════════════════════════
  // Phase 1 — Input Sharing
  // ════════════════════════════════════════════════════════
  PH1_LABEL [label="PHASE 1\nInput Sharing\n(Shamir secret sharing)"
             shape="parallelogram" fillcolor="#e7f3ff" color="#0056b3"
             fontcolor="#0056b3" fontsize="9"]

  // Actions
  P1_dist  [label="Distribute_Share\nSendShare(P₁,Q,mk_share(x₁,r₁))" fillcolor="#e7f3ff" color="#0056b3"]
  P2_recv  [label="Accept_Share\nAuthentic(P₁,P₂,share)\nRecvShare" fillcolor="#e7f3ff" color="#0056b3"]
  P3_recv1 [label="Accept_Share\nAuthentic(P₁,P₃,share)\nRecvShare" fillcolor="#e7f3ff" color="#0056b3"]
  { rank=same; PH1_LABEL; P1_dist }
  { rank=same; P2_recv; P3_recv1 }

  SP11 -> P1_dist  [style="invis"]
  SP21 -> P2_recv  [style="invis"]
  SP31 -> P3_recv1 [style="invis"]

  P1_dist -> P2_recv  [label="senc(<'share', mk_share(x₁,r₁)>, k₁₂)\n✓ share_secrecy  ✓ share_authentication"
                        color="#0056b3" fontcolor="#0056b3"]
  P1_dist -> P3_recv1 [label="senc(<'share', mk_share(x₁,r₁)>, k₁₃)"
                        color="#0056b3" fontcolor="#0056b3"]

  // Adversary observation
  ADV_OBS1 [label="Observes ciphertext\nCannot invert mk_share\n(opaque function)"
            fillcolor="#fff5f5" color="#dc3545" fontsize="8"]
  SA1 -> ADV_OBS1 [style="invis"]
  P1_dist -> ADV_OBS1 [label="sees senc(…)" color="#dc3545" fontcolor="#dc3545"
                        style="dashed" arrowhead="open"]

  // ════════════════════════════════════════════════════════
  // Phase 2 — Beaver Triple Distribution
  // ════════════════════════════════════════════════════════
  PH2_LABEL [label="PHASE 2\nBeaver Triple Distribution\n(one-time, before computation)"
             shape="parallelogram" fillcolor="#d4edda" color="#155724"
             fontcolor="#155724" fontsize="9"]

  D_create [label="Dealer_Create_Triple\n!TriplePool(~a, ~b, ~c)" fillcolor="#d4edda" color="#155724"]
  D_send   [label="Dealer_Distribute_Triple\nDealerSentTriple(Pᵢ,aᵢ,bᵢ,cᵢ)" fillcolor="#d4edda" color="#155724"]
  P1_tri   [label="Party_Receive_Triple\nAuthTriple\nHoldTriple (linear)" fillcolor="#d4edda" color="#155724"]
  P2_tri   [label="Party_Receive_Triple\nAuthTriple\nHoldTriple (linear)" fillcolor="#d4edda" color="#155724"]
  P3_tri   [label="Party_Receive_Triple\nAuthTriple\nHoldTriple (linear)" fillcolor="#d4edda" color="#155724"]
  { rank=same; PH2_LABEL; D_create; D_send }
  { rank=same; P1_tri; P2_tri; P3_tri }

  SD2 -> D_create [style="invis"]
  SD2 -> D_send   [style="invis"]
  SP12 -> P1_tri  [style="invis"]
  SP22 -> P2_tri  [style="invis"]
  SP32 -> P3_tri  [style="invis"]

  D_create -> D_send [style="invis"]
  D_send -> P1_tri [label="senc(<'triple',a₁,b₁,c₁>, k_D1)\n✓ triple_secrecy  ✓ triple_share_authenticity"
                    color="#155724" fontcolor="#155724"]
  D_send -> P2_tri [label="senc(<'triple',a₂,b₂,c₂>, k_D2)" color="#155724" fontcolor="#155724"]
  D_send -> P3_tri [label="senc(<'triple',a₃,b₃,c₃>, k_D3)" color="#155724" fontcolor="#155724"]

  ADV_OBS2 [label="If Corrupt('D'):\nlearns aᵢ,bᵢ,cᵢ\nbreaks triple_secrecy"
            fillcolor="#fff5f5" color="#dc3545" fontsize="8"]
  SA2 -> ADV_OBS2 [style="invis"]
  D_send -> ADV_OBS2 [label="Corrupt(D)→key" color="#dc3545" fontcolor="#dc3545"
                       style="dashed" arrowhead="open"]

  // ════════════════════════════════════════════════════════
  // Phase 3 — Multiplication Broadcast (δ/ε)
  // ════════════════════════════════════════════════════════
  PH3_LABEL [label="PHASE 3\nMultiplication Broadcast\n(δᵢ = xᵢ−aᵢ,  εᵢ = yᵢ−bᵢ)"
             shape="parallelogram" fillcolor="#fff3e0" color="#fd7e14"
             fontcolor="#fd7e14" fontsize="9"]

  P1_mask  [label="Compute_Masked_Values\nconsumes HoldTriple (pop_triple)\n→ !ComputedMask(δ₁,ε₁)" fillcolor="#fff3e0" color="#fd7e14"]
  P1_bcast [label="Broadcast_Masked_Values\nSentMasked(P₁,Pⱼ,δ₁,ε₁)" fillcolor="#fff3e0" color="#fd7e14"]
  P2_rmask [label="Receive_Masked_Values\nAuthMasked(P₂,P₁,δ₁,ε₁)\nHoldMasked" fillcolor="#fff3e0" color="#fd7e14"]
  P3_rmask [label="Receive_Masked_Values\nAuthMasked(P₃,P₁,δ₁,ε₁)\nHoldMasked" fillcolor="#fff3e0" color="#fd7e14"]
  { rank=same; PH3_LABEL; P1_mask; P1_bcast }
  { rank=same; P2_rmask; P3_rmask }

  SP13 -> P1_mask  [style="invis"]
  SP13 -> P1_bcast [style="invis"]
  SP23 -> P2_rmask [style="invis"]
  SP33 -> P3_rmask [style="invis"]

  P1_mask -> P1_bcast [style="invis"]
  P1_bcast -> P2_rmask [label="senc(<'masked',δ₁,ε₁>, k₁₂)\n✓ mul_broadcast_authentication"
                         color="#fd7e14" fontcolor="#fd7e14"]
  P1_bcast -> P3_rmask [label="senc(<'masked',δ₁,ε₁>, k₁₃)" color="#fd7e14" fontcolor="#fd7e14"]

  ADV_OBS3 [label="δᵢ,εᵢ are public after broadcast\n(by design — reveal nothing about xᵢ\nindividually; joint secrecy via shares)"
            fillcolor="#fff5f5" color="#dc3545" fontsize="8"]
  SA3 -> ADV_OBS3 [style="invis"]
  P1_bcast -> ADV_OBS3 [label="can observe\n(by design)" color="#888" fontcolor="#888"
                          style="dashed" arrowhead="open"]

  // ════════════════════════════════════════════════════════
  // Phase 4 — Output Reconstruction
  // ════════════════════════════════════════════════════════
  PH4_LABEL [label="PHASE 4\nOutput Reconstruction\n(Lagrange interpolation)"
             shape="parallelogram" fillcolor="#ede7f6" color="#6f42c1"
             fontcolor="#6f42c1" fontsize="9"]

  P1_out   [label="Broadcast_Output_Share\n!HoldOutputShare(P₁,z₁)\nSentOutputShare" fillcolor="#ede7f6" color="#6f42c1"]
  P2_out   [label="Broadcast_Output_Share\n!HoldOutputShare(P₂,z₂)\nSentOutputShare" fillcolor="#ede7f6" color="#6f42c1"]
  P1_rout  [label="Receive_Output_Share\nAuthOutputShare(P₁,P₂,z₂)\nHoldRecvShare" fillcolor="#ede7f6" color="#6f42c1"]
  P2_rout  [label="Receive_Output_Share\nAuthOutputShare(P₂,P₁,z₁)\nHoldRecvShare" fillcolor="#ede7f6" color="#6f42c1"]
  { rank=same; PH4_LABEL; P1_out; P2_out }
  { rank=same; P1_rout; P2_rout }

  SP14 -> P1_out  [style="invis"]
  SP24 -> P2_out  [style="invis"]
  SP14 -> P1_rout [style="invis"]
  SP24 -> P2_rout [style="invis"]

  P1_out -> P2_rout [label="senc(<'output',z₁>, k₁₂)\n✓ output_share_authentication"
                     color="#6f42c1" fontcolor="#6f42c1"]
  P2_out -> P1_rout [label="senc(<'output',z₂>, k₂₁)" color="#6f42c1" fontcolor="#6f42c1"]

  ADV_OBS4 [label="If Corrupt(Pᵢ):\nlearns zᵢ but not z of others\n(need t shares to reconstruct)"
            fillcolor="#fff5f5" color="#dc3545" fontsize="8"]
  SA4 -> ADV_OBS4 [style="invis"]
  P1_out -> ADV_OBS4 [label="Corrupt(P₁)→key" color="#dc3545" fontcolor="#dc3545"
                       style="dashed" arrowhead="open"]

  // ── Phase ordering (vertical) ─────────────────────────────────────────────
  P1_dist  -> P1_mask  [style="invis"]
  P1_mask  -> P1_out   [style="invis"]
  P2_recv  -> P2_tri   [style="invis"]
  P2_tri   -> P2_rmask [style="invis"]
  P2_rmask -> P2_out   [style="invis"]
  D_create -> D_send   [style="invis"]
  PH1_LABEL -> PH2_LABEL -> PH3_LABEL -> PH4_LABEL [style="invis"]
}
"""

for name, src in [("threat_model_overview", overview), ("threat_model_phases", phases)]:
    dot_path = OUT / f"{name}.dot"
    png_path = OUT / f"{name}.png"
    dot_path.write_text(src.strip())
    result = subprocess.run(
        ["dot", "-Tpng", "-Gdpi=150", str(dot_path), "-o", str(png_path)],
        capture_output=True, text=True
    )
    if result.returncode == 0:
        print(f"rendered {png_path}")
    else:
        print(f"ERROR: {result.stderr}")
