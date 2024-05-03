#![feature(slice_as_chunks)]

use core::{hint::black_box, time::Duration};
use criterion::{criterion_group, criterion_main, Criterion};
use saire::{
    cipher::{TableBlock as OldTableBlock, VirtualPage},
    cipher_::{Cipher, TableBlock as NewTableBlock},
};

fn bench(c: &mut Criterion) {
    let bytes = std::fs::read("./res/toobig.sai").unwrap();
    let table = &bytes[..4096];

    const KEY: u32 = 0;

    // config

    let mut g = c.benchmark_group("decrypt");
    g.warm_up_time(Duration::from_secs(10));

    // benches

    g.bench_function("baseline", |b| {
        b.iter_batched_ref(
            || table.try_into().unwrap(),
            |buf: &mut [u8; 4096]| {
                let cipher = Cipher::<NewTableBlock>::new(KEY);
                cipher.decrypt(buf);
            },
            criterion::BatchSize::SmallInput,
        )
    });

    g.bench_function("old_table_block", |b| {
        let page: [u8; 4096] = table.try_into().unwrap();
        b.iter_batched(
            || page.into(),
            |buf: VirtualPage| black_box(OldTableBlock::decrypt_unchecked(buf, KEY)),
            criterion::BatchSize::SmallInput,
        )
    });

    g.bench_function("new_table_block", |b| {
        b.iter_batched(
            || table.try_into().unwrap(),
            |buf: [u8; 4096]| black_box(<NewTableBlock>::decrypt(KEY, buf)),
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_main!(table_decrypt);
criterion_group!(table_decrypt, bench);
