#!/usr/bin/env python3
"""Rebuild the OFL UI subset: python -m pip install fonttools==4.63.0; see docs/LOCALIZATION.md."""
import argparse,hashlib,json
from pathlib import Path
from fontTools import subset
from fontTools.ttLib import TTFont

ROOT=Path(__file__).resolve().parents[1]
SOURCE_SHA256='2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b'

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('source',type=Path);args=parser.parse_args()
    if hashlib.sha256(args.source.read_bytes()).hexdigest()!=SOURCE_SHA256:raise SystemExit('Unexpected Noto source font; see docs/LOCALIZATION.md')
    cat=json.loads((ROOT/'player/cinder-ui/locales/zh-CN.json').read_text(encoding='utf8'))
    chars=sorted(set(c for c in ''.join(cat.values())+'Language / 语言English简体中文' if 0x3000<=ord(c)<=0x9fff or 0xf900<=ord(c)<=0xfaff or 0xff00<=ord(c)<=0xffef))
    font=TTFont(args.source,recalcTimestamp=False)
    missing=[c for c in chars if ord(c) not in font.getBestCmap()]
    if missing:raise SystemExit('Missing source glyphs: '+''.join(missing))
    options=subset.Options();options.hinting=False;options.layout_features=[];options.name_IDs=[0,1,2,3,4,5,6,13,14]
    sub=subset.Subsetter(options=options);sub.populate(unicodes=list(map(ord,chars)));sub.subset(font)
    for rec in font['name'].names:
        value={1:'Cinder Hans UI',2:'Regular',3:'CinderHansUI-Regular',4:'Cinder Hans UI Regular',6:'CinderHansUI-Regular'}.get(rec.nameID)
        if value:rec.string=value.encode(rec.getEncoding())
    cff=font['CFF '].cff;cff.fontNames[0]='CinderHansUI-Regular';cff.topDictIndex[0].FullName='Cinder Hans UI Regular';cff.topDictIndex[0].FamilyName='Cinder Hans UI'
    dest=ROOT/'player/cinder-ui/assets/fonts/CinderHansUI-Regular.otf';font.save(dest)
    print(json.dumps({'characters':len(chars),'bytes':dest.stat().st_size,'sha256':hashlib.sha256(dest.read_bytes()).hexdigest()}))
