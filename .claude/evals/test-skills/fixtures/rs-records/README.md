# rs-records

Saved notes for a desktop app, kept in `.rec` files that users copy between computers and sometimes edit by hand.

## The file

`b"REC1"`, then a `u16` little-endian count, then each record: a `u8` kind (1 = text, 2 = number), a `u16`
little-endian length, then `length` bytes of payload. Text is UTF-8 of at most 4096 bytes; a number is exactly 8 bytes,
an `i64` little-endian. At most 1000 records. Nothing may follow the last record.

`format::decode` reads a file and refuses anything else with an `Error`; it must never panic, because the files come
from users. `format::encode` writes valid records.

## The store

`store::save(dir, name, records)` writes `<dir>/<name>.rec`. A name is 1 to 64 characters, each an ASCII letter, a
digit, `-` or `_`; any other name is refused with `InvalidInput`. A save is whole or nothing: if the program stops
during a save, the folder holds the previous file or none, never part of one. `store::load(dir, name)` reads it back.
