#!/usr/bin/env python3
"""Copy Muse to the PS Vita's memory card.

    tools/install.py [--volume /Volumes/NAME] [--no-eject]

Put the Vita in VitaShell's USB mode first, so ux0 is mounted. This copies
the built .vpk to ux0:data/, where VitaShell installs it, and the avatar to
ux0:data/musevita/, where the app keeps everything. Then it ejects the card.
Run it again after every rebuild, and install the .vpk again in VitaShell.

This copies the pairing only once. From then on the Vita renews its own
tokens and saves them on the card, so the copy on the card is the live one
and the copy in state/ here goes stale. Overwriting the card's pairing with
a stale one would unpair the Vita, so this leaves an existing pairing alone.
"""
import argparse
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
VPK = ROOT / "target" / "armv7-sony-vita-newlibeabihf" / "release" / "muse-gadget-vita.vpk"
AVATAR = ROOT / "assets" / "avatar" / "avatar.bin"
PAIRING = ["identity.json", "pairing.json"]


def is_ux0(volume):
    return (volume / "app").is_dir() and (volume / "data").is_dir()


def find_card(given):
    if given:
        return pathlib.Path(given)
    roots = [pathlib.Path("/Volumes"), pathlib.Path("/media"), pathlib.Path("/run/media")]
    found = [v for root in roots if root.is_dir() for v in root.glob("*") if is_ux0(v)]
    found += [v for root in roots[1:] if root.is_dir() for v in root.glob("*/*") if is_ux0(v)]
    if len(found) != 1:
        sys.exit("expected one Vita memory card with app and data folders, found %d. "
                 "Put the Vita in VitaShell's USB mode, or pass --volume." % len(found))
    return found[0]


def secret(path_option, default_name):
    path = pathlib.Path(path_option) if path_option else ROOT / default_name
    return path.read_text().strip() if path.exists() else None


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--volume", help="the memory card, ux0 (default: the one mounted)")
    parser.add_argument("--sdk-token-file", help="your Muse SDK token (default: .sdk_token)")
    parser.add_argument("--openai-key-file", help="an OpenAI key for the voice (default: .openai_key)")
    parser.add_argument("--no-eject", action="store_true", help="leave the card mounted")
    args = parser.parse_args()

    if not VPK.exists():
        sys.exit("%s is missing. Build the app first, see the README." % VPK.relative_to(ROOT))
    if not AVATAR.exists():
        sys.exit("%s is missing. Run tools/avatar.sh first, see the README." % AVATAR.relative_to(ROOT))

    card = find_card(args.volume)
    target = card / "data" / "musevita"
    state = target / "state"
    state.mkdir(parents=True, exist_ok=True)

    shutil.copyfile(VPK, card / "data" / VPK.name)
    shutil.copyfile(AVATAR, target / "avatar.bin")

    if (state / "pairing.json").exists():
        print("kept the pairing already on the card")
    elif all((ROOT / "state" / name).exists() for name in PAIRING):
        for name in PAIRING:
            shutil.copyfile(ROOT / "state" / name, state / name)
        print("copied the pairing to the card. The card's copy is the live one from now on.")
    else:
        print("no pairing yet. Muse will show as offline. Run tools/pair.py, then this again.")

    token = secret(args.sdk_token_file, ".sdk_token")
    if token:
        (state / "sdk_token.txt").write_text(token)
    key = secret(args.openai_key_file, ".openai_key")
    if key:
        (state / "openai_key.txt").write_text(key)
    print("voice: %s" % ("on" if (state / "openai_key.txt").exists() else "off, replies will not be spoken"))

    # macOS leaves "._" companions on FAT volumes. Nothing on the Vita needs them.
    for junk in target.rglob("._*"):
        junk.unlink()
    print("copied Muse to %s. Install ux0:data/%s in VitaShell." % (target, VPK.name))

    if args.no_eject:
        return
    subprocess.run(["sync"], check=False)
    if sys.platform == "darwin":
        done = subprocess.run(["diskutil", "eject", str(card)], capture_output=True, text=True)
        print("ejected. Leave USB mode on the Vita." if done.returncode == 0
              else "could not eject %s. Eject it yourself before leaving USB mode.\n%s" % (card, done.stderr.strip()))
    else:
        print("Eject %s before leaving USB mode on the Vita." % card)


if __name__ == "__main__":
    main()
