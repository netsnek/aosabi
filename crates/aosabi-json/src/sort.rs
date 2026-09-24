// SPDX-License-Identifier: MIT
/*
 * aosabi-json sort: the stable sort of Aosabi, one merge sort over indices
 * that a binary holds once.
 *
 * No analogue in the reference, whose sorts run in V8. A site that sorts
 * through it keeps the stable order ECMA-262 asks of Array.prototype.sort,
 * https://tc39.es/ecma262/#sec-sortindexedproperties.
 */

//! `stable_sort_by`, `apply`, `sort_by_stable` and `sort_by_key_stable`.
//!
//! The stable sort of `core` is generic over the element and over the
//! comparison, so a binary holds a copy of its sort for every element type
//! and every comparison that sorts, which
//! `tests/notebooks/17_framework_size.ipynb` counts by element type in its
//! section 3. This module sorts the indices of a slice instead of its
//! elements, through a comparison of two indices that the site hands in as a
//! trait object, so the sort is compiled once a binary, and then permutes the
//! slice by the answer, the one step made for each element type. A stable
//! sort gives one order for a comparison that is a total order, so the answer
//! is the order the stable sort of `core` gives, and
//! `tests/notebooks/18_size_oracles.ipynb` compares the two.
//!
//! A run of at most 20 elements is sorted by insertion, as the stable sort of
//! `core` sorts it, and [`sort_by_stable()`] keeps the indices of such a
//! slice on the stack, so a short sort allocates nothing. A longer run is
//! sorted by its halves, which are merged through a scratch of half its
//! length.

use alloc::vec::Vec;
use core::cmp::Ordering;

/// The longest run sorted by insertion, the bound of the stable sort of `core`.
const INSERTION_MAX: usize = 20;

/// Returns the indices `0..len` in the stable order of `compare`.
///
/// The call `compare(a, b)` compares the elements at the indices `a` and
/// `b`. An index whose element compares equal to that of a later index stays
/// before it, so the answer is the order a stable sort gives, and [`apply()`]
/// permutes a slice of `len` elements by it. The comparison is a trait
/// object, so the sort is compiled once a binary. A comparison that is no
/// total order gives some permutation of the indices and never a panic.
#[must_use]
pub fn stable_sort_by(len: usize, compare: &mut dyn FnMut(usize, usize) -> Ordering) -> Vec<usize> {
    let mut order = alloc::vec![0; len];
    sort_indices(&mut order, compare);
    order
}

/// Permutes `items` by `order`, an answer of [`stable_sort_by()`] for them.
///
/// The element at the index `order[k]` moves to the place `k`, each through
/// a swap along a cycle of the permutation, so no element is copied and
/// nothing is allocated, and every place of `order` holds its own index
/// afterwards. An `order` that is no permutation of the indices of `items`
/// leaves `items` in some order and never panics.
// Erased storage would permute once a binary.
pub fn apply<T>(items: &mut [T], order: &mut [usize]) {
    let len = items.len().min(order.len());
    for start in 0..len {
        let mut at = start;
        while order[at] != at {
            let from = order[at];
            order[at] = at;
            if from >= len || order[from] == from {
                break;
            }
            items.swap(at, from);
            at = from;
        }
    }
}

/// Sorts `items` in the stable order of `compare`, the order `<[T]>::sort_by` gives.
///
/// The call `compare(a, b)` compares two elements, as `sort_by` calls it.
/// The sort runs over the indices of `items` through the merge sort of
/// [`stable_sort_by()`], which a binary holds once, with the indices of at
/// most 20 elements on the stack, and [`apply()`] moves the elements, the one
/// step made for each element type.
pub fn sort_by_stable<T, F>(items: &mut [T], mut compare: F)
where
    F: FnMut(&T, &T) -> Ordering,
{
    if items.len() < 2 {
        return;
    }
    let mut short = [0; INSERTION_MAX];
    let mut long = Vec::new();
    let order = indices(items.len(), &mut short, &mut long);
    sort_indices(order, &mut |a, b| compare(&items[a], &items[b]));
    apply(items, order);
}

/// Sorts `items` in the stable order of the keys `key` gives, the order `<[T]>::sort_by_key` gives.
///
/// The key of each element of a comparison is taken anew, as `sort_by_key`
/// takes it, and the sort is that of [`sort_by_stable()`].
pub fn sort_by_key_stable<T, K, F>(items: &mut [T], mut key: F)
where
    F: FnMut(&T) -> K,
    K: Ord,
{
    sort_by_stable(items, |a, b| key(a).cmp(&key(b)));
}

/// Returns the place for the indices of `len` elements, `short` where they fit and `long` otherwise.
fn indices<'a>(
    len: usize,
    short: &'a mut [usize; INSERTION_MAX],
    long: &'a mut Vec<usize>,
) -> &'a mut [usize] {
    if len <= INSERTION_MAX {
        return &mut short[..len];
    }
    long.resize(len, 0);
    long.as_mut_slice()
}

/// Writes the indices `0..order.len()` into `order` in the stable order of `compare`.
fn sort_indices(order: &mut [usize], compare: &mut dyn FnMut(usize, usize) -> Ordering) {
    for (index, at) in order.iter_mut().enumerate() {
        *at = index;
    }
    let mut scratch = Vec::new();
    if order.len() > INSERTION_MAX {
        scratch.resize(order.len() / 2, 0);
    }
    merge_sort(order, &mut scratch, compare);
}

/// Sorts `order` by its halves and merges them, `scratch` holding at least half of its length.
fn merge_sort(
    order: &mut [usize],
    scratch: &mut [usize],
    compare: &mut dyn FnMut(usize, usize) -> Ordering,
) {
    if order.len() <= INSERTION_MAX {
        insertion_sort(order, compare);
        return;
    }
    let middle = order.len() / 2;
    merge_sort(&mut order[..middle], scratch, compare);
    merge_sort(&mut order[middle..], scratch, compare);
    merge(order, middle, scratch, compare);
}

/// Merges the sorted runs `order[..middle]` and `order[middle..]`, the left run taking each tie.
///
/// The left run moves to `scratch`, and the merge writes from the start of
/// `order`, which never passes the next index of the right run it reads.
fn merge(
    order: &mut [usize],
    middle: usize,
    scratch: &mut [usize],
    compare: &mut dyn FnMut(usize, usize) -> Ordering,
) {
    // Two runs already in order need no merge, so a sorted input costs one
    // comparison per merge.
    if compare(order[middle], order[middle - 1]) != Ordering::Less {
        return;
    }
    let left = &mut scratch[..middle];
    left.copy_from_slice(&order[..middle]);
    let (mut i, mut j, mut k) = (0, middle, 0);
    while i < middle && j < order.len() {
        if compare(order[j], left[i]) == Ordering::Less {
            order[k] = order[j];
            j += 1;
        } else {
            order[k] = left[i];
            i += 1;
        }
        k += 1;
    }
    order[k..k + (middle - i)].copy_from_slice(&left[i..]);
}

/// Sorts `order` by insertion, each index moving before every index whose element is greater.
fn insertion_sort(order: &mut [usize], compare: &mut dyn FnMut(usize, usize) -> Ordering) {
    for i in 1..order.len() {
        let index = order[i];
        let mut j = i;
        while j > 0 && compare(index, order[j - 1]) == Ordering::Less {
            order[j] = order[j - 1];
            j -= 1;
        }
        order[j] = index;
    }
}
