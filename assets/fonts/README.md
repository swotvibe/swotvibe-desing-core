# Reference fonts

These files are the pinned fonts used for deterministic shaping and reference
renders. Tests load the committed bytes directly and never enumerate fonts
installed on the host.

| File | Family | Version | Size | Source pin | SHA-256 |
| --- | --- | --- | ---: | --- | --- |
| `inter/Inter-variable.ttf` | Inter | 4.001 | 876,576 B | `google/fonts` commit `e1d6480102fed30739fead0faee463101f892c8f` | `29160A80FF49DDCAB2C97711247E08B1FAB27A484A329CE8B813D820DC559031` |
| `noto-sans-arabic/NotoSansArabic-variable.ttf` | Noto Sans Arabic | 2.012 | 844,676 B | `google/fonts` commit `660c3b69e347643e4ad6e49f6e600cf1e8d0f7b1` | `63111B5B2E074DD48CC67692E0A2726D86EE94C1C37FE8598257B7B4E87E869E` |

The OFL 1.1 license text is stored beside each font. Its hash is pinned too:

| File | Size | SHA-256 |
| --- | ---: | --- |
| `inter/OFL.txt` | 4,376 B | `5DD548D31A85F756E01D63E00D7FAF1E324103ED3E9102FCBBABF2CC2DB6DD39` |
| `noto-sans-arabic/OFL.txt` | 4,381 B | `DF5143CDF3380169F2D03BF6D2CD243E85621FD097E608C3607CF7F9C9884AE6` |

Inter covers Latin UI labels and interface terms. Noto Sans Arabic covers Arabic
text and its contextual forms; the M0 corpus also contains mixed Arabic and
Latin paragraphs. Both families permit redistribution under SIL OFL 1.1.

## Provenance

- Inter is from the Google Fonts `ofl/inter` directory. The metadata records
  version 4.001 and OFL licensing.
- Noto Sans Arabic is from Google Fonts `ofl/notosansarabic`. Its upstream
  release is 2.012 and the family metadata records OFL licensing.

The source commit, file hash, and license hash together identify the exact test
inputs. Do not replace a font or silently regenerate a golden when any of them
changes; update the fingerprints and obtain a fresh visual review.
