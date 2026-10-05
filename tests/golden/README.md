# Golden files

Approved reference outputs for render and layout comparisons.

Rules:

- Every golden is reviewed by a human before it is committed.
- A comparison records the pinned fonts, backend, and color configuration; a
  backend/font/color change is part of the fingerprint.
- Pixel comparison uses a documented tolerance; different GPUs or operating
  systems are never compared for exact equality.

No image or layout goldens are committed yet. The full M0 gate remains open
until a layout/render path exists and a human-reviewed reference fixture is
available.
