# saire

`saire` ( Sai Reversed Engineered ) is lib that helps to decrypt the file
contents of a .sai file for the SYSTEMAX PaintTool Sai drawing software.

## Acknowledgement

This repository would not have been possible without the research of `Wunkolo`,
the creator of [libsai](https://github.com/Wunkolo/libsai); basically this
library, but on cpp. As of now, the only really differences between the 2
projects is that `saire` can read `Layer` data, but it is still pretty
[limited](#Limitations).

## API Design

For the moment, only a single struct ( `SaiDocument` ) provides access to get
information about a sai file. I will probably provide access to the low level
APIs, but for now I want to clean up the code before doing so.

I might create a `bin` crate later on, but right now it would not be that useful.

## Limitations

This is a list of things that `saire` can't still properly do:

- Decompress layer `mask` data.
- Export to PNG and PSD.
- Some obscure option that I never used...

I'm currently working on understanding how `mask`s work; Then, I want at least
do a `sai to png` exporter.

I don't have plans for the moment to do a `sai to psd` exporter, because that
will be too time consuming, but maybe I get the motivation to do it some day.

Of course, there might be some obscure option in the sai file format, that might
break the library. If you have problems reading a file, please open an issue, I
have tested a couple of files ranging from different dates ( 2014-2019 ), and I
was able to get at least `Layer` data ( didn't test other information ), so in
my end everything seems fine _for now_.

## File Format Specification

If you want a deep dive on how the actually file format works, I will recommend
reading `Wunkolo`'s [document](https://github.com/Wunkolo/libsai#decryption). If
you still have any doubts you can read `saire` source code or open an issue
here.
