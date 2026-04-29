//! Integration tests for the mean-pooling operation in `graphtor_core::embed`.
//!
//! These tests use synthetic `candle_core` tensors and do not require model
//! weights — they run unconditionally in `cargo test`.

use candle_core::{Device, Tensor};
use graphtor_core::embed::mean_pool;

/// All tokens are real (mask = 1).  The mean of the hidden states equals the
/// arithmetic mean of the values.
#[test]
fn mean_pool_all_real_tokens() {
    let device = Device::Cpu;

    // batch=1, seq_len=3, hidden=2
    // row 0: [1.0, 2.0], row 1: [3.0, 4.0], row 2: [5.0, 6.0]
    // expected mean: [(1+3+5)/3, (2+4+6)/3] = [3.0, 4.0]
    let hidden_data: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let hidden =
        Tensor::from_vec(hidden_data, (1usize, 3usize, 2usize), &device).expect("hidden tensor");

    let mask_data: Vec<u32> = vec![1, 1, 1];
    let mask = Tensor::from_vec(mask_data, (1usize, 3usize), &device).expect("mask tensor");

    let result = mean_pool(&hidden, &mask).expect("mean_pool failed");
    let values = result
        .squeeze(0)
        .expect("squeeze")
        .to_vec1::<f32>()
        .expect("to_vec1");

    assert_eq!(values.len(), 2);
    assert!(
        (values[0] - 3.0_f32).abs() < 1e-5,
        "dim0 expected 3.0, got {}",
        values[0]
    );
    assert!(
        (values[1] - 4.0_f32).abs() < 1e-5,
        "dim1 expected 4.0, got {}",
        values[1]
    );
}

/// Padding is present: the last token position is masked out.
/// Only the first two token representations contribute to the mean.
#[test]
fn mean_pool_partial_mask_ignores_padding() {
    let device = Device::Cpu;

    // batch=1, seq_len=3, hidden=2
    // rows: [2.0, 4.0], [6.0, 8.0], [100.0, 100.0]  <- last row is padding
    // mask: [1, 1, 0]
    // expected mean: [(2+6)/2, (4+8)/2] = [4.0, 6.0]
    let hidden_data: Vec<f32> = vec![2.0, 4.0, 6.0, 8.0, 100.0, 100.0];
    let hidden =
        Tensor::from_vec(hidden_data, (1usize, 3usize, 2usize), &device).expect("hidden tensor");

    let mask_data: Vec<u32> = vec![1, 1, 0];
    let mask = Tensor::from_vec(mask_data, (1usize, 3usize), &device).expect("mask tensor");

    let result = mean_pool(&hidden, &mask).expect("mean_pool failed");
    let values = result
        .squeeze(0)
        .expect("squeeze")
        .to_vec1::<f32>()
        .expect("to_vec1");

    assert_eq!(values.len(), 2);
    assert!(
        (values[0] - 4.0_f32).abs() < 1e-5,
        "dim0 expected 4.0, got {}",
        values[0]
    );
    assert!(
        (values[1] - 6.0_f32).abs() < 1e-5,
        "dim1 expected 6.0, got {}",
        values[1]
    );
}
