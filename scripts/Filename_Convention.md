README – CNH Histology Microscopy Filename Convention
====================================================

General Template
----------------
[DS][Block]_[Cond]_SL[Slide]_IM[Img]_[Stain]_[Pair]_[Obj]_[Focus]_[Date]_[Initials].tif

Example:
03p_Cad_SL47_IM01_BF-DAPI_G[BF]_20x_HD_20260419_CNH


Field Definitions
-----------------

1. Dataset / Block ID: [DS][Block]
   - DS = embedding round number (2 digits recommended)
     - Example: 02 = second round of embedding
   - Block = letter indicating block in that round
     - Example: 02f = second round of embedding, block f
   - If > 26 blocks in a single round, continue with double letters
     - Example: 02aa = round 2, block aa

2. Condition: [Cond]
   - E   = Exposed
   - Z   = Zombie
   - C   = Uninfected / Control
   - Cad = Cadaver
   - D#  = Day number (e.g., D4, D7)

3. Slide: SL[Slide]
   - Each slide has a single unique number.
   - Example: SL45, SL047

4. Image / Series Number: IM[Img]
   - Image index from that slide.
   - Example: IM01, IM02

5. Stains / Dyes: [Stain]
   - List stains, no commas; separate with hyphens.
   - Standard codes:
     - BF         = brightfield
     - DAPI
     - Phalloidin
     - Calcofluor
     - PI         = propidium iodide
     - FISH
     - Sytox
     - Hoescht
   - Example (triple stain): Phalloidin-Sytox-DAPI

6. Pair / Group ID: [Pair]
   - Used when channels from the **same field of view** are saved as separate images.
   - Use G[...] to indicate which channel the file represents in that group.
   - Example with PI, DAPI, and BF from the same FOV:
     - DAPI channel image → G[DAPI]
     - PI channel image   → G[PI]
     - Brightfield image  → G[BF]
   - All images from the same FOV share the same DS/Block, Cond, Slide, Img, Obj, Focus, Date, and Initials, 
     and differ by Stain and G[...] label.

7. Objective: [Obj]
   - Magnification of objective lens.
   - Examples: 10x, 20x, 40x, 60x

8. Focus / Region: [Focus]
   - Shorthand for specific region imaged.
   - WB  = whole body
   - HD  = head
   - BR  = brain
   - ABD = abdomen
   - THX = thorax

9. Date: [Date]
   - Format: YYYYMMDD
   - Example: 20260425

10. Initials: [Initials]
    - CNH


Full Example (Explained)
------------------------

Filename:
03p_Cad_SL47_IM01_BF-DAPI_G[BF]_20x_HD_20260419_CNH

Meaning:
- 03p      → Cadaver from block p of the 3rd round of embedding
- Cad      → Condition = Cadaver
- SL47     → Slide number 47
- IM01     → First image from that slide
- BF-DAPI  → Image includes brightfield and DAPI
- G[BF]    → This file is the brightfield image in that group/series
- 20x      → 20x objective
- HD       → Head region
- 20260419 → April 19, 2026
- CNH      → Your initials