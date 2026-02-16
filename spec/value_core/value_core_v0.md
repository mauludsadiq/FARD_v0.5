# Value Core v0 (Normative)

## 1. Runtime value set V

V contains exactly:

- null
- bool
- int
- string
- list
- rec
- bytes
- func, builtin (runtime-only; not JSON-serializable)

## 2. Canonical JSON encoding (current runtime)

All values encode to "raw JSON" except bytes.

- null: JSON null
- bool: JSON boolean
- int: JSON number (signed i64 domain)
- string: JSON string
- list: JSON array of values
- rec: JSON object; keys are strings; order is implementation-defined by serde (not relied on for semantics)
- bytes: JSON object {"t":"bytes","v":"hex:<lowercase-hex>"}

### 2.1 bytes invariants

- {"t":"bytes","v":...} is reserved for bytes decoding.
- v MUST begin with the literal prefix "hex:".
- hex payload MUST be lowercase [0-9a-f], even length, no separators.
- decoding is exact: bytes are the hex-decoded payload.

## 3. Numeric / overflow rules (int)

- int is signed i64 at runtime.
- overflow MUST raise ERROR_OVERFLOW deterministically.
