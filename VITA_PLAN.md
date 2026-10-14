# Muse on a PS Vita

A native PS Vita app that turns a PlayStation Vita into a [Muse gadget](https://gadgets.muse.ai).

Hold the right shoulder button (or touch the screen), talk into the built-in
microphone, and Muse answers out loud through the speaker while its avatar
listens, thinks and speaks on screen.

Everything runs on the Vita. The app holds its own encrypted session with Muse
over Wi-Fi, sends your voice as a voice note, and speaks the reply. It is
written in Rust, and the Muse client is the platform-independent `link/`
crate, which follows the protocol of the [Muse Gadget SDK](https://github.com/facebookincubator/muse-gadget-sdk).

This is a port of [wobsoriano/muse-gadget-psp](https://github.com/wobsoriano/muse-gadget-psp)
(Muse on a Sony PSP, Apache 2.0). The network/Muse core (`link/`, `host/`,
`tests/`, `tools/`) is carried over nearly untouched; only the frontend in
`vita/` is new, written against VitaSDK instead of the PSP APIs.

> This is a hobby project. It is not affiliated with, endorsed by, or
> supported by Sony or Meta. PlayStation and PS Vita are trademarks of Sony
> Interactive Entertainment. Muse is a product of Meta. You use this at your
> own risk, including the custom firmware it needs.

## What you need

- **A PS Vita** (any model — every one has a built-in microphone) with
  **custom firmware** (HENkaku / h-encore, ideally Enso) and VitaShell.
- **A Muse account, the Muse app on your phone, and an SDK token** from
  [gadgets.muse.ai](https://gadgets.muse.ai).
- **A computer** to build the app (Linux or macOS; Windows via WSL2).
- **A second device with Bluetooth LE, for pairing** — once. The Vita's
  Bluetooth is 2.1+EDR, which has no Low Energy mode, and Muse pairs a
  gadget over BLE. A Raspberry Pi or a Linux computer with the SDK's Linux
  client does the one Bluetooth conversation; you then hand the resulting
  identity and tokens to the Vita. See *Pair it with Muse*.
- **An OpenAI API key, for the voice.** Without one the Vita still hears you
  and Muse still answers, but the Vita stays silent. (Muse sends text, not
  audio — something has to speak it.)

## Install

### 1. Get the tools

Install [VitaSDK](https://vitasdk.org) (see `vdpm`), then Rust nightly and
the Vita target:

```
rustup toolchain install nightly
rustup target add armv7-sony-vita-newlibeabihf --toolchain nightly
cargo +nightly install cargo-vita
```

### 2. Make the avatar

The app plays the avatar that ships with the Muse Gadget SDK. That avatar is
Meta's and the SDK's licence does not cover it, so it is not in this
repository. Build it from your own copy of the SDK:

```
git clone https://github.com/facebookincubator/muse-gadget-sdk vendor/muse-gadget-sdk
tools/avatar.sh
```

This runs the SDK's own renderer on your computer and writes
`assets/avatar/avatar.bin`, one short clip for each mood. Git ignores both
the clone and the output.

### 3. Build

```
cargo +nightly vita build vpk --release
```

This produces `target/armv7-sony-vita-newlibeabihf/release/muse-gadget-vita.vpk`.

`vita/src/sys.rs` declares the VitaSDK functions the app uses. Every
signature marked TODO there should be checked against the VitaSDK headers
on the first real build — they are best-effort guesses, not verified
headers.

### 4. Pair it with Muse

Muse pairs a gadget over Bluetooth LE, which the Vita cannot do. So a
device that can does that conversation once. What comes out of it is small,
an identity and a pair of tokens, and you hand those to the Vita. After
that the Vita connects to Muse by itself and renews its own credentials.

Any Muse gadget client that has finished pairing will do. The Linux client
in the Muse Gadget SDK is the simplest:

1. Install it on a Raspberry Pi or a Linux computer with Bluetooth LE, and
   pair it from the Muse app, following its README.
2. Stop it, so it is no longer using the pairing:
   `sudo systemctl disable --now musegadget`
3. Copy `identity.json` and `pairing.json` out of `/var/lib/musegadget` to
   your computer.
4. Hand them to this repository:

```
tools/pair.py path/to/the/folder
```

The tool saves the pairing in `state/`, which git ignores. Treat it like a
password.

Two rules matter here. Only one device can use an identity at a time, and
every renewal replaces the tokens. So once the Vita has the pairing, leave
the client it came from switched off, or remove it.

### 5. Copy it to the Vita

Put your SDK token in a file named `.sdk_token` in this folder, and an
OpenAI key in `.openai_key`. Git ignores both.

Put the Vita in VitaShell's **USB mode** (so `ux0` is mounted), then:

```
tools/install.py
```

It copies the `.vpk` to `ux0:/data/` — install it in VitaShell — and the
avatar, pairing and keys to `ux0:/data/musevita/`, where the app keeps
everything. It never overwrites a pairing that is already on the card.

## Use it

Join Wi-Fi in the Vita's Settings, make sure the date and time are set, and
run **Muse** from the LiveArea.

Hold the **right shoulder button** (or keep a finger on the touchscreen) to
talk, release to send. Press **PS** to quit to LiveArea.

The corner of the screen shows whether the Vita can reach Muse, and the
avatar shows what it is doing:

| You see | It means |
| --- | --- |
| An amber light and "connecting" | Joining Wi-Fi or reaching Muse |
| A green light and "online" | Ready. Hold R to talk. |
| A red light and "offline" | Muse has been out of reach for 45 seconds, or the Vita is not paired. The app keeps trying. |
| The avatar in blue | Listening while you hold R |
| The avatar in pink, with thought dots | Your question is on its way, or Muse is answering |
| The avatar in green, with sound waves | Speaking the answer |
| Hearts for a few seconds, no sound | Muse answered and the Vita could not speak it |

**What Muse can do to the Vita.** Ask Muse from any device:

| Command | What it does |
| --- | --- |
| `vita.say` | Speaks a message through the Vita |
| `vita.dance` | Makes the avatar dance |
| `vita.status` | Reads the battery and Wi-Fi |

Name the device when you ask, for example "say hello on my Vita".

## How it works

```
vita/     the Vita app: screen, avatar, microphone, speaker, Wi-Fi, storage
link/     the Muse client and the speech request, with no Vita code in them
host/     programs that run link/ on a computer
tools/    the avatar, pairing, installing
tests/    the client against a fake Muse
```

- **The Muse client** (`link/`) fetches your Muse's address over HTTPS,
  opens a WebSocket, runs a Noise XX handshake inside it, registers the
  Vita, and then serves commands and questions. It uses rustls for TLS 1.3
  and has no threads of its own. The app calls it in a loop. This crate is
  `no_std` and platform-independent — the PSP port and this port share it
  unchanged.
- **A voice turn.** The app records at 22,050 Hz while you hold the talk
  button and sends it to Muse as a WAV voice note. Muse answers in text.
  The app sends that text to OpenAI and plays the audio at 44,100 Hz while
  it is still arriving.
- **Pairing without Bluetooth LE.** The Vita copies `identity.json` and
  `pairing.json` from a Linux client that paired once; from then on it
  renews its own tokens.
- **The log.** The app writes `ux0:/data/musevita/log.txt` (and keeps the
  previous run as `log-before.txt`). Read those first when something
  misbehaves.

## Develop

`link/` builds for your computer, which is where its tests run:

```
cd link && cargo test && cd ..
uv run --with pytest --with cryptography --with websockets pytest tests/test_rust_session.py
```

Two programs in `host/` run the Vita's network code against the real
services from a computer. `host-check` makes one HTTPS request, and
`speech` speaks a sentence and saves the audio.

## Known limits

- **Untested on hardware.** This is a scaffold: it is written against
  VitaSDK from documentation, and every `TODO` in `vita/src/sys.rs` needs
  checking on a real build. Expect the first hardware run to find mistakes.
- **Pairing needs a second device.** There is no way around the missing
  Bluetooth LE.
- **Keys sit in plain text on the memory card.** That includes the pairing
  and the OpenAI key. Anyone holding the card can read them.
- **The random numbers are weaker than a modern device's.** Microphone
  noise is the best source this app currently uses.
- **No voice without OpenAI.** There is no offline voice. When the Vita
  cannot speak an answer, all you get is the hearts.
- **Long answers get cut short.** About the first 420 characters are spoken.
- **Muse sends text, not audio.** OpenAI makes the voice.

## License

Apache License 2.0. See LICENSE, and NOTICE for the third-party work this
builds on. The avatar built by `tools/avatar.sh` is Meta's and is not
covered by this repository's licence.
