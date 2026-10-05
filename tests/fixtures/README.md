# Test fixtures

This directory is reserved for versioned test and product-reference documents.
No representative customer documents are currently committed. Tests generate
small synthetic documents; they are not a substitute for a licensed product
corpus.

Planned contents:

- Valid documents across every supported schema version (migration inputs)
- Malformed and adversarial documents for parser fuzzing
- Round-trip documents that must open and re-save without unintended change

When adding fixtures, record their source/schema version and license. Keep
adversarial parser inputs separate from valid product-reference documents.
