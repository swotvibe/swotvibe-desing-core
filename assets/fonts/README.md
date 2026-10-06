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

## Unused faces kept in the repository

These two files are **not read by any test or fixture**. They are recorded here
so that a committed asset has a stated source and license, and so that nobody
assumes they influence a measurement. The upstream release commit has not been
verified against Google Fonts, which is part of why they are not reference
inputs: a face used by a reference render has to be pinned to a verifiable
revision.

| File | Family | Version | Size | SHA-256 |
| --- | --- | --- | ---: | --- |
| `noto-latin/NotoSans-Regular.ttf` | Noto Sans | 2.015 | 41,508 B | `4D5A883E71623B58390D3E1CDDF78EB84F0CA1F43D0CC3974DE1B284AD8FACFC` |
| `noto-naskh-arabic/NotoNaskhArabic-Regular.ttf` | Noto Naskh Arabic | 2.021 | 199,512 B | `706B4580C025F6C75EC1C06013E17F00A23EC1780091B77A5A5B339469BDA94D` |

Both embed an SIL OFL 1.1 notice, and the license text is stored beside each file:

| File | Size | SHA-256 |
| --- | ---: | --- |
| `noto-latin/OFL.txt` | 4,396 B | `CEE9892F9F0CC8FE882C9E9537EE6A89621D86EE7CEAF70B02E2B2B1C25C061A` |
| `noto-naskh-arabic/OFL.txt` | 4,382 B | `A7A5A25EB188BF1CD96982030D53E23C33485C69B1044A562254226857EE13AF` |

They are not substitutes for the pinned faces in `inter/` and `noto-sans-arabic/`:
the families differ, and even where a family matches, a different revision moves
every measurement.

Either pin them the way the table at the top of this file pins the reference
faces — source commit, verified hash, and a test that registers them — or remove
the directories. Until then they stay unused.

