//! The byte ranges a tag parser would read and throw away (ID3v2 `APIC` frame
//! bodies, FLAC `PICTURE` blocks), mapped before it runs so `BlockReader`
//! serves them as zeros without fetching them.
