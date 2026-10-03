// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-bluetooth host: the seam of a bare metal LE host, core only.
 *
 * No analogue on the reference side.
 */

//! The seam of a bare metal LE host.
//!
//! A card that runs the carrier on bare metal hands the bindings of `bare`
//! an LE host: one that listens for the channel on an SPSM with the MPS and
//! the credits of the card and answers each accepted channel as a byte
//! stream, publishes the PSM in its characteristic, and moves the packets of
//! the GATT data path, each written value with the index of its central and
//! each notification to one central. The Linux driver of this crate reaches
//! `BlueZ` instead and implements no such host.

use homa::stream::ByteStream;

/// The LE host of a bare metal card.
// The seam is called by the executor of the card alone, whose futures need no
// `Send`, so the bound the lint asks a public trait to state has no user.
#[expect(
    async_fn_in_trait,
    reason = "the executor of a bare metal card runs one thread and needs no Send bound"
)]
pub trait LeHost {
    /// An accepted channel, a byte stream of prefixed `AoFrame`.
    type Channel: ByteStream;
    /// A failure of the host.
    type Error;

    /// Listens for the channel on `spsm` with the MPS `mps` and `credits`
    /// initial credits, and returns the next accepted channel.
    async fn listen_channel(
        &mut self,
        spsm: u16,
        mps: u16,
        credits: u16,
    ) -> Result<Self::Channel, Self::Error>;

    /// Publishes `spsm` in the PSM characteristic of the service.
    async fn publish_psm(&mut self, spsm: u16) -> Result<(), Self::Error>;

    /// Takes the next value a central wrote to RX into `buf` and returns the
    /// index of the central and the length of the value.
    async fn gatt_rx(&mut self, buf: &mut [u8]) -> Result<(u32, usize), Self::Error>;

    /// Notifies `packet` on TX to the central `central`.
    async fn gatt_tx(&mut self, central: u32, packet: &[u8]) -> Result<(), Self::Error>;

    /// Returns the `ATT_MTU` of the link to the central `central`.
    fn att_mtu(&self, central: u32) -> u16;
}
