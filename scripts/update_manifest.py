"""Writes the two small files the apps read to find out whether a newer version is out:
fluora-update.json and renamer-update.json.

Used by the GitHub build (.github/workflows/build.yml) after the installers are made.
  python3 update_manifest.py <folder with the release's .sig files> <version> <out folder>

Each file says: the version, and for Mac and Windows where to download the update and the
signature that proves it came from this build.
"""
import datetime
import json
import pathlib
import sys

REPO = "charliechuckcharles/Fluora"


def manifest(sigs, version, prefix):
    """prefix is how the app's files start on the release page: 'Fluora_' or 'Fluora.Renamer_'."""
    base = f"https://github.com/{REPO}/releases/download/v{version}/"
    mine = {p.name[:-4]: p.read_text().strip() for p in sigs.glob("*.sig") if p.name.startswith(prefix)}

    def pick(ending):
        names = [n for n in mine if n.endswith(ending)]
        if len(names) != 1:
            sys.exit(f"Expected one file starting {prefix!r} and ending {ending!r}, found {names or 'none'}")
        return {"signature": mine[names[0]], "url": base + names[0]}

    mac, nsis, msi = pick(".app.tar.gz"), pick("-setup.exe"), pick(".msi")
    return {
        "version": version,
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": {
            # one Mac build runs on both Apple-chip and Intel Macs
            "darwin-aarch64": mac, "darwin-x86_64": mac,
            # Windows: update with the same kind of installer the person used
            "windows-x86_64": nsis, "windows-x86_64-nsis": nsis, "windows-x86_64-msi": msi,
        },
    }


if __name__ == "__main__":
    sigs, version, out = pathlib.Path(sys.argv[1]), sys.argv[2].lstrip("v"), pathlib.Path(sys.argv[3])
    out.mkdir(parents=True, exist_ok=True)
    for name, prefix in (("fluora", "Fluora_"), ("renamer", "Fluora.Renamer_")):
        (out / f"{name}-update.json").write_text(json.dumps(manifest(sigs, version, prefix), indent=2) + "\n")
        print("wrote", name + "-update.json")
