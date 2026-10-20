#!/usr/bin/env python3
"""Rebuild the OFL CJK UI subset from the languages marked cjk_font: pip install fonttools==4.63.0; docs/LOCALIZATION.md."""
import argparse,hashlib,json
from pathlib import Path
from fontTools import subset
from fontTools.ttLib import TTFont

ROOT=Path(__file__).resolve().parents[1]
SOURCE_SHA256='2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b'

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('source',type=Path);args=parser.parse_args()
    if hashlib.sha256(args.source.read_bytes()).hexdigest()!=SOURCE_SHA256:raise SystemExit('Unexpected Noto source font; see docs/LOCALIZATION.md')
    loc=ROOT/'player/cinder-ui/locales';langs=json.loads((loc/'languages.json').read_text(encoding='utf8'))
    # Every language marked cjk_font, plus every language's name and word for the Settings row.
    text=''.join(''.join(json.loads((loc/f"{l['tag']}.json").read_text(encoding='utf8')).values()) for l in langs if l.get('cjk_font'))
    text+=''.join(l['name']+l['word'] for l in langs)
    chars=sorted(set(c for c in text if 0x3000<=ord(c)<=0x9fff or 0xf900<=ord(c)<=0xfaff or 0xff00<=ord(c)<=0xffef))
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
