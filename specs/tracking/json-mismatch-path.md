# A JSON mismatch names its path — `compiler.md` §194

Status: contract committed. Implementation is next.

## Measurement at the pin `3231e6d7`

The probe is one `Person` class (`name: string`, `age: u8`,
`friend: Person | null`) and four documents. The probe ran on the dev
tier with `target/debug/subscript run`, built from the pin.

| Case | Document | Output |
|---|---|---|
| nested range | `{"name":"a","age":1,"friend":{"name":"b","age":300,"friend":null}}` | `JSON.parse: document does not match Person` |
| nested kind | `{"name":"a","age":1,"friend":5}` | `JSON.parse: document does not match Person` |
| nested missing | `{"name":"a","age":1,"friend":{"name":"b","friend":null}}` | `JSON.parse: document does not match Person` |
| root kind | `{"name":7,"age":1,"friend":null}` | `JSON.parse: document does not match Person` |

The four failures give one message. The committed golden of
`corpus/accept/a255-json-parse-direct` shows the same fact for
`missing field` and `wrong kind`.

## Prior art

scriptc reports a checked-cast mismatch with a path and the two kinds:
`expected number at $.port, got string` *(docs:
<https://github.com/vercel-labs/scriptc>, the "Runtime type checks"
limitation page)*. No scriptc binary was run for this note.
