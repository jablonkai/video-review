"""Static font instances for egui (it can't pick variable-font axes or OpenType
features). Tabular figures are frozen into the cmap so timecode never jitters.

    uv run --with fonttools tools/make_fonts.py <flutter-app>/assets/fonts assets/fonts
"""
import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

src, dst = Path(sys.argv[1]), Path(sys.argv[2])

ARCHIVO = {
    "Archivo-400": dict(wght=400, wdth=100),
    "Archivo-600": dict(wght=610, wdth=100),
    "Archivo-700n": dict(wght=700, wdth=85),
    "ArchivoTC-300": dict(wght=300, wdth=82),
    "ArchivoTC-450": dict(wght=450, wdth=82),
    "ArchivoTC-520": dict(wght=520, wdth=82),
    "ArchivoTC-600": dict(wght=600, wdth=82),
}
SERIF = {"SourceSerif-420": dict(wght=420, opsz=14), "SourceSerif-display": dict(wght=400, opsz=24)}


def freeze(font, tag):
    if "GSUB" not in font:
        return
    gsub = font["GSUB"].table
    mapping = {}
    for rec in gsub.FeatureList.FeatureRecord:
        if rec.FeatureTag != tag:
            continue
        for li in rec.Feature.LookupListIndex:
            lk = gsub.LookupList.Lookup[li]
            for st in lk.SubTable:
                if lk.LookupType == 7:
                    st = st.ExtSubTable
                if hasattr(st, "mapping"):
                    mapping.update(st.mapping)
    for table in font["cmap"].tables:
        for cp, g in list(table.cmap.items()):
            if g in mapping:
                table.cmap[cp] = mapping[g]
    return len(mapping)


def make(file, name, axes, tnum):
    f = TTFont(src / file)
    inst = instantiateVariableFont(f, axes)
    n = freeze(inst, "tnum") if tnum else 0
    inst.save(dst / f"{name}.ttf")
    print(name, "tnum glyphs:", n)


for name, axes in ARCHIVO.items():
    make("Archivo.ttf", name, axes, True)
for name, axes in SERIF.items():
    make("SourceSerif4.ttf", name, axes, False)
