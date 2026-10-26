# Embedded

Aosabi is meant to answer GraphQL on a board with no operating system, over the links a board has.
This release builds the core crates for the ESP32-C3. The work here brings the whole engine onto
boards and onto small Linux devices, and measures it there.

## 1. The engine on boards

1. The engine built with `core` and `alloc` alone for the ESP32-C3, the ESP32-C6 and the ESP32-S3,
   with every form a board needs under a configuration that no host sets, so every host keeps its
   features and its bytes.
2. The run time of a request on a board, a resolver that awaits among them, on a core with no
   threads and no atomic operations of pointer width.
3. The first image of a service for the ESP32-C3, with esp-hal on bare metal and embassy as the
   executor, answering over the built in USB serial, a serial line and Bluetooth LE.
4. Every number of the embedded guide measured again on the board itself, the bytes of the image,
   the RAM of a request and the time of an answer.

Parts with about 2 KB of RAM stay outside, because a parser, a validator and a JSON codec do not fit
there.

## 2. Small Linux devices

The static binary for `mipsel-unknown-linux-musl` builds today. The work runs it on a device,
measures its resident memory against a budget of 12 MiB, and serves GraphTP over USB and Bluetooth
from the device beside HTTP.

## 3. Device cards

A device card is one PNG picture of a device that packs everything a build needs for it: a manifest,
the source of its board support and drivers, its configuration, what is known about its hardware,
the licenses and the signatures. A build takes the card and needs nothing else for that device.

1. The format of a card and the command line to pack, inspect, verify and extract one.
2. Signatures, a trust store and revocation, so no code of a card runs on a build host without a
   trusted signature.
3. A registry that stores cards by their digest and serves a static index, as a convenience for
   finding cards and never as a dependency of a build.
4. The build of a service from a card, the flashing of its image, and a chapter on the device in the
   generated documentation.

## 4. LoRa

The carrier LoRa frames one sealed Homa packet per LoRa frame and keeps the duty cycle in a ledger,
and today it runs over a model of two radios. The work serves GraphTP over a real radio, the SX1262,
between two boards.
