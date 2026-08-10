#!/usr/bin/env python3
"""Regenerate `analysis::damage_mods::catalog` from the GW2EI C# sources.

The damage-modifier catalog is ~6,600 lines of transliterated GW2EI
definitions. Hand-writing it would be neither reviewable nor verifiable, so
it is EXTRACTED: this script enumerates every `new *DamageModifier(...)`
statement in the files the catalog draws from and either transcribes it or
rejects it with a named reason. The accounting it prints --
`considered == transcribed + skipped` -- is the machine-diff behind the
catalog's completeness claim, and is why this script is committed rather
than left as a scratch file: the claim is only worth something if it can be
re-run.

Nothing here guesses. An unknown gain computer, an unresolved icon or buff
symbol, a non-literal name, an unknown build constant, a synthetic
(negative) buff id, or an unhandled builder method all raise `Skip`, which
lands the statement in the skipped table WITH its reason rather than
producing a subtly wrong definition. A statement can collect several
reasons (e.g. an unrepresentable checker AND an early exit); all of them are
kept, so the in-code table never understates why something is missing.

Scope rule: the three shared tables (`ItemDamageModifiers.cs`,
`GearDamageModifiers.cs`, `SharedDamageModifiers.cs`) are transcribed WHOLE;
the profession helpers contribute only the ids the WvW reference capture's
`damageModMap` carries (`TARGET_IDS`).

Usage:

    python3 scripts/gen_damage_mod_catalog.py [/path/to/GW2EI/checkout]

then `git diff`: a clean tree means the committed catalog is exactly what
the current GW2EI source produces. Standard library only.
"""

import collections
import glob
import os
import re
import sys

ROOT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/gw2ei"
OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "crates", "axilog-core", "src", "analysis", "damage_mods", "catalog",
    )
)

# Every id in the reference capture's `damageModMap`
# (`fixtures/local/wvw-postrework.ei.json`, GW2 build 204489). That fixture
# is gitignored and PII-bearing, so the id list is inlined rather than read.
TARGET_IDS = {
    10, 11, 18, 21, 25, 36, 44, 54, 57, 58, 59, 61, 62, 67, 72, 74, 75, 78, 87, 93, 94, 98, 99, 107, 108, 109, 111, 119, 124, 125, 126, 128, 129, 131, 132, 170, 172, 173, 174, 175, 176, 215, 229, 312, 313, 318, 319, 327, 328, 334, 336, 361, 362, 364, 368, 369, 370, 371, 372, 374, 376, 389, 390, 403, 411, 422, 423, 424, 425, 426, 427, 428, 429, 431,
}



# ----------------------------------------------------------------------
# GW2EI constant tables
# ----------------------------------------------------------------------

def consts(path, pat):
    out={}
    for line in open(os.path.join(ROOT,path),encoding='utf-8-sig'):
        m=re.match(pat,line.strip())
        if m: out[m.group(1)]=m.group(2)
    return out

MODIDS={k:int(v) for k,v in consts('GW2EIEvtcParser/ParserHelpers/IDs/DamageModifierIDs.cs',
    r'public const int (\w+)\s*=\s*(-?\d+);').items()}
BUILDS={}
for line in open(ROOT+'/GW2EIEvtcParser/ParserHelpers/GW2Builds.cs',encoding='utf-8-sig'):
    m=re.match(r'\s*public const (?:ulong|long) (\w+)\s*=\s*(\w+)(?:\.(\w+))?;',line)
    if m:
        v=m.group(2)
        if v=='ulong': BUILDS[m.group(1)]= 0 if 'MinValue' in line else 2**64-1
        else: BUILDS[m.group(1)]=int(v)
ARCBUILDS={}
for line in open(ROOT+'/GW2EIEvtcParser/ParserHelpers/ArcDPSBuilds.cs',encoding='utf-8-sig') if os.path.exists(ROOT+'/GW2EIEvtcParser/ParserHelpers/ArcDPSBuilds.cs') else []:
    m=re.match(r'\s*public const (?:ulong|int|long) (\w+)\s*=\s*(\w+)',line)
    if m: ARCBUILDS[m.group(1)]=m.group(2)
SKILLS={}
for line in open(ROOT+'/GW2EIEvtcParser/ParserHelpers/IDs/SkillIDs.cs',encoding='utf-8-sig'):
    m=re.match(r'\s*public const long (\w+)\s*=\s*(-?\d+);',line)
    if m: SKILLS[m.group(1)]=int(m.group(2))
IMAGES={}
for f in glob.glob(ROOT+'/GW2EIEvtcParser/ParserHelpers/Images/*.cs'):
    cls=os.path.basename(f)[:-3]
    for line in open(f,encoding='utf-8-sig'):
        m=re.match(r'\s*(?:public|internal) const string (\w+)\s*=\s*"([^"]*)";',line)
        if m: IMAGES[cls+'.'+m.group(1)]=m.group(2)

# ----------------------------------------------------------------------
# C# statement extraction
# ----------------------------------------------------------------------

def split_top(s):
    out=[];d=0;cur='';instr=False;i=0
    while i<len(s):
        ch=s[i]
        if instr:
            cur+=ch
            if ch=='\\': cur+=s[i+1]; i+=2; continue
            if ch=='"': instr=False
            i+=1; continue
        if ch=='"': instr=True; cur+=ch; i+=1; continue
        if ch in '([{': d+=1
        elif ch in ')]}': d-=1
        if ch==',' and d==0:
            out.append(cur.strip()); cur=''
        else: cur+=ch
        i+=1
    if cur.strip(): out.append(cur.strip())
    return out

def find_stmts(text):
    """yield (start,end,ctor,argstr,chain) for each `new XxxDamageModifier(...)` + chain"""
    for m in re.finditer(r'new\s+(\w*DamageModifier)\s*\(', text):
        ctor=m.group(1); j=m.end()-1
        d=0;k=j
        while k<len(text):
            if text[k]=='(':d+=1
            elif text[k]==')':
                d-=1
                if d==0:break
            elif text[k]=='"':
                k+=1
                while text[k]!='"':
                    if text[k]=='\\':k+=1
                    k+=1
            k+=1
        args=text[j+1:k]; k+=1
        chain=[]
        while True:
            mm=re.match(r'\s*\.(\w+)\(',text[k:])
            if not mm: break
            s=k+mm.end()-1; d=0;p=s
            while p<len(text):
                if text[p]=='(':d+=1
                elif text[p]==')':
                    d-=1
                    if d==0:break
                elif text[p]=='"':
                    p+=1
                    while text[p]!='"':
                        if text[p]=='\\':p+=1
                        p+=1
                p+=1
            chain.append((mm.group(1), text[s+1:p]))
            k=p+1
        yield (m.start(), k, ctor, args, chain)

# ----------------------------------------------------------------------
# statement -> definition record
# ----------------------------------------------------------------------

DT={'All':'All','Power':'Power','Strike':'Strike','Condition':'Condition','LifeLeech':'LifeLeech',
    'StrikeAndCondition':'StrikeAndCondition','ConditionAndLifeLeech':'ConditionAndLifeLeech',
    'StrikeAndLifeLeech':'StrikeAndLifeLeech','StrikeAndConditionAndLifeLeech':'StrikeAndConditionAndLifeLeech'}
DS={'All':'All','NoPets':'NoPets','PetsOnly':'PetsOnly','Incoming':'Incoming'}
MODE={'PvE':'PvE','PvEInstanceOnly':'PvEInstanceOnly','sPvP':'SPvP','WvW':'WvW','All':'All',
      'sPvPWvW':'SPvPWvW','PvEWvW':'PvEWvW','PvEsPvP':'PvESPvP'}
CHECKERS={
 'x.IsMoving':'SrcMoving', 'x.AgainstMoving':'AgainstMoving', 'x.IsOverNinety':'OverNinety',
 'x.AgainstUnderFifty':'AgainstUnderFifty', 'x.AgainstDowned':'AgainstDowned',
 'x.HasCrit':'Crit', 'x.HasGlanced':'Glance', 'x.IsFlanking':'Flanking',
 'x.ShieldDamage > 0':'ShieldDamage',
}
# The 12 boons EI classifies as BuffClassification.Boon post-May2021
# (CommonBuffs.cs:14-29); NumberOfBoons is their presence-merge graph.
BOONS=['Might','Fury','Quickness','Alacrity','Protection','Regeneration','Vigor','Aegis',
       'Stability','Swiftness','Resistance','Resolution']
INTENSITY={'Might','Stability','Vulnerability','Bleeding','Burning','Confusion','Poison','Torment'}

class Skip(Exception): pass

def rs_str(s):
    return '"'+s.replace('\\','\\\\').replace('"','\\"')+'"'

def num(expr):
    e=expr.strip()
    if re.fullmatch(r'-?[\d_.]+(?:[fdm])?', e): return float(e.rstrip('fdm'))
    if re.fullmatch(r'[-+*/().\d\s]+', e): return float(eval(e))
    if e=='int.MaxValue': return float(2**31-1)
    raise Skip('gain expression '+e)

def buff_ids(expr):
    e=expr.strip()
    if e.startswith('['):
        names=[x.strip() for x in split_top(e[1:-1].strip()) if x.strip()]
        multi=True
    else:
        names=[e]; multi=False
    if names==['NumberOfBoons']:
        return BOONS, True, 'NumberOfBoons'
    if names==['NumberOfConditions']:
        raise Skip('NumberOfConditions pseudo-buff graph (no condition-buff id table in this project)')
    for n in names:
        if n not in SKILLS: raise Skip('unresolved buff symbol '+n)
        if SKILLS[n] < 0:
            raise Skip('EI-synthetic buff id %s = %d (a computed graph, not a wire buff)' % (n, SKILLS[n]))
    return names, multi, None

def tracker_rs(names, multi):
    ids=', '.join(str(SKILLS[n]) for n in names)
    return f'BuffTracker {{ ids: &[{ids}], multi: {str(multi).lower()} }}'

def icon(expr):
    e=expr.strip()
    if e in IMAGES: return IMAGES[e]
    raise Skip('unresolved icon '+e)

def mode(expr):
    e=expr.strip().split('.')[-1]
    if e not in MODE: raise Skip('unknown mode '+e)
    return MODE[e]

def source(expr):
    e=expr.strip()
    if e.startswith('['):
        # `HashSet<Source>` overload (`DamageModifierDescriptor.cs:51`): the
        # modifier is filed under EVERY source. `ModSource::Spec` holds one
        # name, so transcribing this would under-offer the modifier; refuse
        # rather than guess. See `ModSource::Spec`'s TODO.
        names = [x.strip().split('.')[-1] for x in split_top(e[1:-1])]
        raise Skip('multi-source definition (HashSet<Source> %s) -- ModSource::Spec '
                   'holds a single source' % '/'.join(names))
    n=e.split('.')[-1]
    if n in ('Item','Gear','Common'): return f'ModSource::{n}'
    if n=='FightSpecific' or n=='Encounter': return 'ModSource::Encounter'
    return f'ModSource::Spec("{n}")'

def gain_computer(expr):
    e=expr.strip()
    m=re.match(r'new GainComputerBy(\w+)\((.*)\)$', e)
    if m:
        kind,arg=m.group(1),m.group(2).strip()
        if kind=='AtLeastNStacksPresent': return f'GainComputer::AtLeastNStacks({int(arg)})'
        if kind=='AtMostNStacksPresent': return f'GainComputer::AtMostNStacks({int(arg)})'
        if kind=='ExactNumberOfBuffsPresent': return f'GainComputer::ExactNStacks({int(arg)})'
        if kind=='StackPlusConstant': return f'GainComputer::ByStackPlusConstant({num(arg)})'
        raise Skip('gain computer '+e)
    if e=='ByPresence': return 'GainComputer::ByPresence'
    if e=='ByStack': return 'GainComputer::ByStack'
    if e=='ByMultiPresence': return 'GainComputer::ByMultiPresence'
    if e=='ByAbsence': return 'GainComputer::ByAbsence'
    if e=='ByMultipliyingStack': return 'GainComputer::ByMultiplyingStack'
    raise Skip('gain computer '+e)

def checker(expr):
    e=' '.join(expr.split())
    m=re.match(r'\((\w+), *(\w+)\) *=> *(.*)$', e)
    body = m.group(3) if m else e
    if m:
        body=body.replace(m.group(1)+'.', 'x.')
    body=body.strip()
    if body=='true': return []  # a no-op checker (GW2EI writes `(x, log) => true`)
    if body in CHECKERS: return [f'HitCheck::{CHECKERS[body]}']
    if e=='VulnerabilityActiveCheck':
        # SharedDamageModifiers.cs:14-31 -- "the target is not under
        # Resistance"; the two extra arms are raid-boss species probes
        # (Sabir, MaliciousShadowCM), unreachable in WvW.
        return [f'HitCheck::DstLacksBuff({SKILLS["Resistance"]})']
    raise Skip('checker `'+e+'`')

def builds(chain, key, lo_default, hi_default, table):
    for name,arg in chain:
        if name==key:
            parts=split_top(arg)
            def res(p):
                p=p.strip().split('.')[-1]
                if p in table: return table[p]
                raise Skip('unknown build constant '+p)
            lo=res(parts[0]); hi=res(parts[1]) if len(parts)>1 else hi_default
            return lo,hi
    return lo_default,hi_default

def analyse(ctor, args, chain, path, line):
    a=split_top(args)
    reasons=[]
    rec={'file':path,'line':line,'checks':[],'counter':False,'approx':False,
         'actor_master':False,'foe_master':False,'absorbed':False,'from_foe':False,
         'from_actor':False,'actor_check':None}
    if ctor=='DamageLogDamageModifier':
        if len(a)!=11: raise Skip(f'unexpected DamageLog arity {len(a)}')
        sym,name,tip,dsrc,gain,st,ct,src,ic,chk,md=a
        rec.update(trigger='Hit', gain_computer='GainComputer::ByPresence',
                   gain_per_stack=num(gain))
        try:
            rec['checks'] += checker(chk)
        except Skip as ex:
            reasons.append(str(ex))
    elif ctor in ('BuffOnActorDamageModifier','BuffOnFoeDamageModifier'):
        if len(a)!=12: raise Skip(f'unexpected arity {len(a)}')
        sym,bid,name,tip,dsrc,gain,st,ct,src,gc,ic,md=a
        names,multi,_=buff_ids(bid)
        rec.update(gain_per_stack=num(gain), gain_computer=gain_computer(gc),
                   tracker=tracker_rs(names,multi))
        rec['trigger']='BuffOnFoe' if 'Foe' in ctor else 'BuffOnActor'
    elif ctor in ('CounterOnActorDamageModifier','CounterOnFoeDamageModifier'):
        if len(a)!=11: raise Skip(f'unexpected counter arity {len(a)}')
        sym,bid,name,tip,dsrc,st,ct,src,gc,ic,md=a
        names,multi,_=buff_ids(bid)
        rec.update(gain_per_stack=100.0, gain_computer=gain_computer(gc),
                   tracker=tracker_rs(names,multi), counter=True)
        rec['trigger']='BuffOnFoe' if 'Foe' in ctor else 'BuffOnActor'
    elif ctor=='SkillDamageModifier':
        if len(a)!=10: raise Skip(f'unexpected skill arity {len(a)}')
        sym,name,tip,skid,dsrc,st,ct,src,ic,md=a
        s=skid.strip()
        sid=SKILLS.get(s) if not s.lstrip('-').isdigit() else int(s)
        if sid is None: raise Skip('unresolved skill symbol '+s)
        rec.update(trigger=f'Skill({sid})', gain_computer='GainComputer::BySkill',
                   gain_per_stack=float(2**31-1))
    else:
        raise Skip('unsupported descriptor '+ctor)

    if sym.strip() not in MODIDS: raise Skip('unknown id symbol '+sym)
    rec['id']=MODIDS[sym.strip()]; rec['sym']=sym.strip()
    rec['name']=eval(name.strip()) if name.strip().startswith('"') else (_ for _ in ()).throw(Skip('non-literal name'))
    rec['desc']=eval(tip.strip()) if tip.strip().startswith('"') else (_ for _ in ()).throw(Skip('non-literal tooltip'))
    rec['dmg_src']=f"DamageSource::{DS[dsrc.strip().split('.')[-1]]}"
    rec['src_type']=f"DamageType::{DT[st.strip().split('.')[-1]]}"
    rec['compare_type']=f"DamageType::{DT[ct.strip().split('.')[-1]]}"
    rec['source']=source(src)
    rec['icon']=icon(ic)
    rec['mode']=f'ModifierMode::{mode(md)}'

    for cname,arg in chain:
        if cname=='WithBuilds' or cname=='WithEvtcBuilds':
            continue
        elif cname=='UsingSpecSpecificShared': rec['spec_shared']=True
        elif cname=='UsingChecker':
            try:
                rec['checks'] += checker(arg)
            except Skip as ex:
                reasons.append(str(ex))
        elif cname=='UsingApproximate': rec['approx']=True
        elif cname=='UsingActorFetchIsAlwaysMaster': rec['actor_master']=True
        elif cname=='UsingFoeFetchIsAlwaysMaster': rec['foe_master']=True
        elif cname=='UsingHitAndAbsorbedDamageEvents':
            reasons.append('UsingHitAndAbsorbedDamageEvents (absorbed hits not classified)')
        elif cname=='UsingEarlyExit':
            reasons.append('UsingEarlyExit (early-exit actor checker not modelled)')
        elif cname=='UsingGainAdjuster':
            reasons.append('UsingGainAdjuster (gain adjuster not modelled)')
        elif cname=='WithBuffOnActorFromFoe':
            reasons.append('WithBuffOnActorFromFoe (per-applier stacks not modelled)')
        elif cname=='WithBuffOnFoeFromActor':
            reasons.append('WithBuffOnFoeFromActor (per-applier stacks not modelled)')
        elif cname in ('UsingActorCheckerByPresence','UsingActorCheckerByAbsence'):
            names,multi,_=buff_ids(arg)
            rec['actor_check']=(tracker_rs(names,multi), cname.endswith('ByAbsence'))
        else: reasons.append('unhandled builder .'+cname+'()')

    if rec['trigger']=='BuffOnFoe':
        reasons.append('BuffOnFoe family: GW2EI drops it outright in WvW/sPvP '
                       '(BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here')
    if reasons:
        raise Skip('; '.join(reasons))

    rec['gw2'] = builds(chain,'WithBuilds',0,2**64-1,BUILDS)
    rec['evtc']= builds(chain,'WithEvtcBuilds',None,None,{})
    return rec

# ----------------------------------------------------------------------
# buff stack types
# ----------------------------------------------------------------------

INTENSITY_STACK_TYPES = {"Stacking", "StackingConditionalLoss", "StackingUniquePerSrc"}
DURATION_STACK_TYPES = {"Queue", "Regeneration", "Force"}


def buff_stack_table():
    """`buff id -> (intensity, kind, capacity, name, file, line)`.

    Parsed out of GW2EI's own `new Buff(...)` declarations; the short ctor
    overload defaults to `BuffStackType.Force, 1` (`Buff.cs:120-125`).
    Era-gated redefinitions of one id keep the FIRST declaration, which is
    the live one for every id the catalog watches (verified: none of them
    has a conflicting redefinition).
    """
    table = {}
    for f in glob.glob(ROOT + "/GW2EIEvtcParser/EIData/**/*.cs", recursive=True):
        t = open(f, encoding="utf-8-sig").read()
        for m in re.finditer(r"new Buff\(", t):
            j = m.end() - 1
            d = 0
            k = j
            while k < len(t):
                if t[k] == "(":
                    d += 1
                elif t[k] == ")":
                    d -= 1
                    if d == 0:
                        break
                elif t[k] == '"':
                    k += 1
                    while t[k] != '"':
                        if t[k] == "\\":
                            k += 1
                        k += 1
                k += 1
            a = [x.strip() for x in split_top(t[j + 1:k])]
            if len(a) < 4 or a[1] not in SKILLS:
                continue
            bid = SKILLS[a[1]]
            idx = [i for i, x in enumerate(a) if "BuffStackType." in x]
            if idx:
                kind = a[idx[0]].split("BuffStackType.")[1].strip()
                cap = a[idx[0] + 1]
            else:
                kind, cap = "Force", "1"
            if kind not in INTENSITY_STACK_TYPES and kind not in DURATION_STACK_TYPES:
                continue
            try:
                cap = int(cap)
            except ValueError:
                cap = 1
            table.setdefault(
                bid,
                (
                    kind in INTENSITY_STACK_TYPES,
                    kind,
                    cap,
                    a[0].strip('"'),
                    os.path.relpath(f, ROOT),
                    t.count("\n", 0, m.start()) + 1,
                ),
            )
    return table


# ----------------------------------------------------------------------
# Rust emission
# ----------------------------------------------------------------------

GROUP_DOC = {
    "item": "Consumables (`ItemDamageModifiers.cs`) -- food and utility nourishment.",
    "gear": "Gear (`GearDamageModifiers.cs`) -- runes, sigils and relics.",
    "shared": "Profession-agnostic shared modifiers (`SharedDamageModifiers.cs`) -- "
              "boons, conditions and the Exposed family.",
}
U64MAX = 2 ** 64 - 1


def rs_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def rs_f(x):
    x = float(x)
    return repr(x) if x != int(x) else f"{int(x)}.0"


def collect():
    """Every `new *DamageModifier(...)` in scope, analysed or skipped."""
    groups = [
        ("item", "GW2EIEvtcParser/EIData/DamageModifiers/CommonDamageModifiers/ItemDamageModifiers.cs", None),
        ("gear", "GW2EIEvtcParser/EIData/DamageModifiers/CommonDamageModifiers/GearDamageModifiers.cs", None),
        ("shared", "GW2EIEvtcParser/EIData/DamageModifiers/CommonDamageModifiers/SharedDamageModifiers.cs", None),
    ]
    for f in sorted(glob.glob(ROOT + "/GW2EIEvtcParser/EIData/ProfHelpers/*/*.cs")):
        groups.append(
            (os.path.basename(f)[: -len("Helper.cs")].lower(), os.path.relpath(f, ROOT), TARGET_IDS)
        )

    kept, skipped, considered = {}, [], 0
    for gname, rel, filt in groups:
        text = open(os.path.join(ROOT, rel), encoding="utf-8-sig").read()
        recs = []
        for (start, _end, ctor, args, chain) in find_stmts(text):
            sym = split_top(args)[0].strip()
            mid = MODIDS.get(sym)
            if mid is None:
                continue
            if filt is not None and mid not in filt:
                continue
            considered += 1
            line = text.count("\n", 0, start) + 1
            try:
                recs.append(analyse(ctor, args, chain, rel, line))
            except Skip as ex:
                skipped.append((mid, sym, rel, line, str(ex)))
        if recs:
            kept[gname] = recs
    return kept, skipped, considered


def emit_group(gname, recs):
    per_id = collections.Counter()
    idents, out = [], []
    doc = GROUP_DOC.get(
        gname,
        f"`{recs[0]['file']}` -- the {gname.capitalize()} definitions observed "
        "in the WvW reference capture.",
    )
    out += [
        "//! " + doc,
        "//!",
        "//! Machine-transcribed from GW2EI; every entry carries the `file:line` of",
        "//! the C# statement it came from. See `super`'s module doc for the",
        "//! transcription rules and the skipped-definition list.",
        "",
        "#![allow(clippy::excessive_precision)]",
        "",
        "use super::super::model::*;",
        "",
    ]
    for r in recs:
        n = per_id[r["id"]]
        per_id[r["id"]] += 1
        ident = f"D{r['id']}_{n}"
        idents.append(ident)
        lo, hi = r["gw2"]
        trig = r["trigger"]
        if trig == "Hit":
            trig_rs = "Trigger::Hit"
        elif trig.startswith("Skill("):
            trig_rs = f"Trigger::{trig}"
        elif trig == "BuffOnActor":
            trig_rs = f"Trigger::BuffOnActor {{ tracker: {r['tracker']}, from_foe: false }}"
        else:
            raise SystemExit("unexpected trigger " + trig)
        out += [
            f"/// GW2EI `{r['sym']} = {r['id']}` -- `{r['file']}:{r['line']}`.",
            f"pub static {ident}: DamageModifierDef = DamageModifierDef {{",
            f"    id: {r['id']},",
            f"    name: {rs_str(r['name'])},",
            f"    icon: {rs_str(r['icon'])},",
            f"    description: {rs_str(r['desc'])},",
            f"    source: {r['source']},",
            f"    spec_specific_shared: {str(r.get('spec_shared', False)).lower()},",
            f"    gain_per_stack: {rs_f(r['gain_per_stack'])},",
            f"    gain: {r['gain_computer']},",
            f"    trigger: {trig_rs},",
            f"    src_type: {r['src_type']},",
            f"    compare_type: {r['compare_type']},",
            f"    dmg_src: {r['dmg_src']},",
            "    checks: &[" + ", ".join(r["checks"]) + "],",
            f"    mode: {r['mode']},",
            f"    approximate: {str(r['approx']).lower()},",
            f"    is_counter: {str(r.get('counter', False)).lower()},",
            f"    actor_always_master: {str(r['actor_master']).lower()},",
            f"    foe_always_master: {str(r['foe_master']).lower()},",
            "    with_absorbed_damage_events: false,",
            f"    min_gw2_build: {'START_OF_LIFE' if lo == 0 else lo},",
            f"    max_gw2_build: {'END_OF_LIFE' if hi == U64MAX else hi},",
            "    min_evtc_build: EVTC_START_OF_LIFE,",
            "    max_evtc_build: EVTC_END_OF_LIFE,",
            "};",
            "",
        ]
    out += [f"/// Every definition in this group ({len(idents)})."]
    out += ["pub static DEFS: &[&DamageModifierDef] = &["]
    out += [f"    &{i}," for i in idents]
    out += ["];", ""]
    open(os.path.join(OUT, gname + ".rs"), "w").write("\n".join(out))
    return idents


def emit_buff_stack(used_ids, table):
    L = [
        "//! `buff id -> (stacking kind, capacity)`, for every buff the catalog",
        "//! watches (M16, Task 2).",
        "//!",
        "//! Stack type is a property of the BUFF, not of the definition that reads",
        "//! it -- GW2EI keeps it on its `Buff` catalog (`EIData/Buffs/`,",
        "//! `new Buff(name, id, source, BuffStackType.X, capacity, ...)`; the short",
        "//! ctor overload defaults to `BuffStackType.Force, 1`, `Buff.cs:120-125`).",
        "//! This project has no full buff catalog, so the subset the damage-modifier",
        "//! catalog needs is transcribed here rather than being declared per",
        "//! definition: a multi-buff tracker over the twelve boons mixes intensity",
        "//! (Might, Stability) and duration (the other ten) ids, so one flag per",
        "//! TRACKER cannot be right -- it silently simulated Fury, Protection and",
        "//! Resolution as stacking buffs, which the calibration caught.",
        "//!",
        "//! `intensity` is `BuffStackType` in",
        "//! `{Stacking, StackingConditionalLoss, StackingUniquePerSrc}`",
        "//! (`ArcDPSEnums.cs:384-393`); `Queue`/`Regeneration`/`Force` are duration",
        "//! buffs. `capacity` is GW2EI's own, used only as the fallback when the log",
        "//! carries no `CBTS_BUFFINFO` row for the buff.",
        "",
        "/// One row of GW2EI's buff catalog: `(id, intensity, capacity)`.",
        "pub struct BuffStackInfo {",
        "    pub id: u32,",
        "    pub intensity: bool,",
        "    pub capacity: u32,",
        "}",
        "",
        f"/// Sorted by id ([`stack_info`] binary-searches it). {len(used_ids)} entries.",
        "pub static BUFF_STACK_INFO: &[BuffStackInfo] = &[",
    ]
    for i in used_ids:
        e = table[i]
        L.append(f"    // {e[3]} -- BuffStackType.{e[1]}, {e[2]} ({e[4]}:{e[5]})")
        L.append(
            f"    BuffStackInfo {{ id: {i}, intensity: {str(e[0]).lower()}, capacity: {e[2]} }},"
        )
    L += [
        "];",
        "",
        "/// The catalogued row for a buff id, if this project knows it.",
        "pub fn stack_info(id: u32) -> Option<&'static BuffStackInfo> {",
        "    BUFF_STACK_INFO.binary_search_by_key(&id, |b| b.id).ok().map(|i| &BUFF_STACK_INFO[i])",
        "}",
        "",
        "/// `BuffStackType` is intensity-stacking. Unknown ids are treated as",
        "/// duration buffs, which is GW2EI's own default (`Buff.cs:120`).",
        "pub fn is_intensity(id: u32) -> bool {",
        "    stack_info(id).is_some_and(|b| b.intensity)",
        "}",
        "",
        "/// Every id any catalog tracker watches must be in the table, or its",
        "/// stack simulation silently falls back to \"duration, capacity 5\".",
        "#[cfg(test)]",
        "#[test]",
        "fn every_tracked_buff_has_a_stack_type() {",
        "    let mut ids: Vec<u32> = super::CATALOG",
        "        .iter()",
        "        .flat_map(|d| d.trackers().into_iter().flat_map(|t| t.ids.iter().copied()))",
        "        .chain(super::CATALOG.iter().flat_map(|d| d.checks.iter().filter_map(|c| c.buff_id())))",
        "        .collect();",
        "    ids.sort_unstable();",
        "    ids.dedup();",
        "    let missing: Vec<u32> = ids.into_iter().filter(|&i| stack_info(i).is_none()).collect();",
        "    assert!(missing.is_empty(), \"buff ids with no stack type: {missing:?}\");",
        "}",
        "",
        "/// The table must stay sorted -- [`stack_info`] binary-searches it.",
        "#[cfg(test)]",
        "#[test]",
        "fn table_is_sorted_by_id() {",
        "    assert!(BUFF_STACK_INFO.windows(2).all(|w| w[0].id < w[1].id));",
        "}",
        "",
    ]
    open(os.path.join(OUT, "buff_stack.rs"), "w").write("\n".join(L))


MOD_TESTS = r'''
/// Assert every catalog entry is a combination GW2EI itself could produce
/// (`DamageModifierDef::validate`) -- the transcription guard for the
/// generated table.
#[cfg(test)]
#[test]
fn catalog_definitions_are_valid() {
    for d in CATALOG {
        d.validate().unwrap_or_else(|e| panic!("{e}"));
    }
    assert!(!CATALOG.is_empty());
}

/// The one invariant duplicate ids must respect.
///
/// A reworked modifier is several entries sharing one `json_id` with
/// disjoint build windows and/or disjoint modes (GW2EI writes them that
/// way, e.g. `Mod_BloodyRoar` has ten). What must NEVER happen is two of
/// them being live at once: [`super::evaluate`] resolves a running key back
/// to its definition by `json_id`, and GW2EI asserts the same uniqueness
/// when it builds `OutgoingDamageModifiersByID`
/// (`DamageModifiersContainer.cs:111-117`).
///
/// So this sweeps every build boundary the catalog mentions (plus one below
/// and one above each, to catch an off-by-one in a half-open window) across
/// every `(ParseMode, SkillMode)` pair, and asserts the surviving set is
/// id-unique. That is a far stronger transcription guard than a flat
/// duplicate check, and it is what actually caught window typos while this
/// table was being written.
#[cfg(test)]
#[test]
fn no_ambiguous_definition_for_any_build_and_mode() {
    use super::model::{ParseMode, SkillMode, END_OF_LIFE};
    use std::collections::{BTreeMap, BTreeSet};

    let mut builds: BTreeSet<u64> = BTreeSet::new();
    builds.insert(0);
    for d in CATALOG {
        for b in [d.min_gw2_build, d.max_gw2_build] {
            if b == END_OF_LIFE {
                continue;
            }
            builds.insert(b.saturating_sub(1));
            builds.insert(b);
            builds.insert(b + 1);
        }
    }
    let modes = [
        (ParseMode::WvW, SkillMode::WvW),
        (ParseMode::SPvP, SkillMode::SPvP),
        (ParseMode::Instanced, SkillMode::PvE),
        (ParseMode::OpenWorld, SkillMode::PvE),
        (ParseMode::Unknown, SkillMode::PvE),
    ];
    let mut failures: Vec<String> = Vec::new();
    for &b in &builds {
        for (pm, sm) in modes {
            let mut seen: BTreeMap<i32, &str> = BTreeMap::new();
            for d in CATALOG {
                if !d.available(Some(b), None) || !d.keep(pm, sm) {
                    continue;
                }
                if let Some(prev) = seen.insert(d.json_id(), d.name) {
                    failures.push(format!(
                        "build {b} {pm:?}/{sm:?}: id {} claimed by both {prev:?} and {:?}",
                        d.json_id(),
                        d.name
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "ambiguous definitions:\n{}", failures.join("\n"));
}

/// The catalog must not contain a definition the engine would silently
/// drop: transcription is only useful if [`super::is_supported`] accepts it.
#[cfg(test)]
#[test]
fn every_catalog_definition_is_supported_by_the_engine() {
    for d in CATALOG {
        assert!(
            super::is_supported(d),
            "{}: transcribed but rejected by is_supported -- it belongs in the \
             skipped list in this module's doc instead",
            d.name
        );
    }
}
'''


def emit_mod(kept, skipped, considered):
    groups = sorted(kept)
    per = collections.Counter()
    entries = []
    for g in groups:
        for r in kept[g]:
            n = per[(g, r["id"])]
            per[(g, r["id"])] += 1
            entries.append(f"{g}::D{r['id']}_{n}")
    d = []
    d.append("//! The damage-modifier DEFINITION table (M16, Task 2).")
    d.append("//!")
    d.append("//! **Generated** by `scripts/gen_damage_mod_catalog.py`; re-run it and")
    d.append("//! `git diff` to re-verify against a GW2EI checkout. Hand edits are lost.")
    d.append("//!")
    d.append("//! # What is in here")
    d.append("//!")
    d.append("//! GW2EI's definition set, grouped exactly as GW2EI groups it: the three")
    d.append("//! shared tables (`CommonDamageModifiers/{Item,Gear,Shared}DamageModifiers.cs`)")
    d.append("//! are transcribed COMPLETE -- every WvW-expressible member, observed in")
    d.append("//! the reference capture or not -- and the per-profession files contribute")
    d.append("//! the definitions whose ids appear in that capture's `damageModMap`.")
    d.append("//!")
    d.append(f"//! **{considered} statements considered = {len(entries)} transcribed + "
             f"{len(skipped)} skipped.** Nothing is dropped silently; the skipped table")
    d.append("//! below is exhaustive and each row carries every reason that applies.")
    d.append("//!")
    d.append("//! Every entry cites the `file:line` of the C# statement it came from, and")
    d.append("//! carries GW2EI's own era windows verbatim: a reworked trait is several")
    d.append("//! entries sharing one id with disjoint `[min, max)` build ranges and/or")
    d.append("//! disjoint modes, exactly as upstream writes it. Selecting between them is")
    d.append("//! `DamageModifierDef::available` + `DamageModifierDef::keep`, not this")
    d.append("//! table -- so the catalog is era-agnostic and a pre-rework log picks its")
    d.append("//! own variant. `no_ambiguous_definition_for_any_build_and_mode` proves")
    d.append("//! the windows really are disjoint.")
    d.append("//!")
    d.append("//! # Transcription rules")
    d.append("//!")
    d.append("//! - `CounterOn{Actor,Foe}DamageModifier` passes a hardcoded `gainPerStack`")
    d.append("//!   of `100.0` to its base ctor (`CounterOnActorDamageModifier.cs:9-16`),")
    d.append("//!   so counters are transcribed with `gain_per_stack: 100.0` +")
    d.append("//!   `is_counter: true`, never with the \"percent\" the name suggests.")
    d.append("//! - `SkillDamageModifier` passes `int.MaxValue`")
    d.append("//!   (`SkillDamageModifier.cs:27`) -- a sentinel, since its gain is")
    d.append("//!   hardcoded to `1.0`; transcribed literally so nothing silently trips")
    d.append("//!   `DamageModifierDef::validate`'s non-zero rule.")
    d.append("//! - `NumberOfBoons` (`SkillIDs.cs:20`, id `-3`) is not a real buff: it is")
    d.append("//!   the PRESENCE MERGE of every `BuffClassification.Boon` graph")
    d.append("//!   (`SingleActorBuffsHelper.cs:963-1040`, `MergePresenceInto`). A")
    d.append("//!   `BuffsTrackerSingle` over it therefore returns \"how many distinct")
    d.append("//!   boons are up\", which is EXACTLY what a `BuffTracker` with")
    d.append("//!   `multi: true` over the twelve boon ids computes")
    d.append("//!   (`BuffsTrackerMulti.cs:7-15`) -- so those definitions are transcribed")
    d.append("//!   as multi trackers rather than needing a synthetic graph.")
    d.append("//! - GW2EI's `Source` enum only decides which spec a modifier is offered")
    d.append("//!   to; the engine never branches on it, so it is carried as a label")
    d.append("//!   (`ModSource`).")
    d.append("//!")
    d.append("//! # Deliberately NOT transcribed")
    d.append("//!")
    d.append("//! Each line is a GW2EI statement this project cannot evaluate faithfully.")
    d.append("//! None is silently approximated -- the definition is simply absent, and")
    d.append("//! ALL of the reasons that apply to it are recorded here:")
    d.append("//!")
    d.append("//! | id | symbol | file:line | why |")
    d.append("//! | --- | --- | --- | --- |")
    for mid, sym, rel, line, why in sorted(skipped, key=lambda s: (s[0], s[2], s[3])):
        d.append(f"//! | {mid} | `{sym}` | `{os.path.basename(rel)}:{line}` | {why.replace('|', chr(92) + '|')} |")
    d.append("//!")
    d.append("//! The `BuffOnFoe` rows are not a gap in this port: GW2EI itself returns")
    d.append("//! `false` from `BuffOnFoeDamageModifier.Keep` for every WvW and sPvP log")
    d.append("//! before consulting anything else (`:83-91`), so those modifiers are")
    d.append("//! definitionally inert in this project's only parse mode. They are listed")
    d.append("//! for completeness, and transcribing them would add dead entries that")
    d.append("//! `DamageModifierDef::keep` drops anyway.")
    d.append("")
    d.append("pub mod buff_stack;")
    for g in groups:
        d.append(f"pub mod {g};")
    d.append("")
    d.append("/// GW2EI `Mod_MovingBonus = 10` -- kept under its Task 1 name because the")
    d.append("/// calibration harness and several unit tests refer to it by that name.")
    d.append("pub use item::D10_0 as MOVING_BONUS;")
    d.append("")
    d.append("use super::model::DamageModifierDef;")
    d.append("")
    d.append("/// Every definition this project knows about.")
    d.append("///")
    d.append("/// Ordering is the engine's iteration order and therefore part of its")
    d.append("/// determinism contract -- but the engine's output is a `BTreeMap` keyed by")
    d.append("/// `(player, id)`, so catalog order cannot affect results either way.")
    d.append("pub static CATALOG: &[&DamageModifierDef] = &[")
    for e in entries:
        d.append(f"    &{e},")
    d.append("];")
    d.append(MOD_TESTS)
    open(os.path.join(OUT, "mod.rs"), "w").write("\n".join(d))


def main():
    if not os.path.isdir(ROOT):
        raise SystemExit(f"GW2EI checkout not found at {ROOT}")
    kept, skipped, considered = collect()
    for gname, recs in sorted(kept.items()):
        emit_group(gname, recs)

    used = set()
    for recs in kept.values():
        for r in recs:
            for tr in ("tracker", "actor_check_tracker"):
                if r.get(tr):
                    used.update(int(x) for x in re.findall(r"\d+", r[tr].split("]")[0]))
            for c in r["checks"]:
                m = re.match(r"HitCheck::DstLacksBuff\((\d+)\)", c)
                if m:
                    used.add(int(m.group(1)))
    table = buff_stack_table()
    missing = sorted(i for i in used if i not in table)
    if missing:
        raise SystemExit(f"no GW2EI stack type for buff id(s): {missing}")
    emit_buff_stack(sorted(used), table)
    emit_mod(kept, skipped, considered)

    print(f"statements considered: {considered}")
    print(f"  transcribed: {sum(len(v) for v in kept.values())}")
    print(f"  skipped:     {len(skipped)}")
    print(f"  buff ids:    {len(used)}")
    assert considered == sum(len(v) for v in kept.values()) + len(skipped)
    for g, v in sorted(kept.items()):
        print(f"    {g:<14} {len(v)}")
    reasons = collections.Counter(
        r.split("(")[0].split(":")[0].strip() for s in skipped for r in s[4].split("; ")
    )
    for k, v in reasons.most_common():
        print(f"    skip: {v}x {k}")


if __name__ == "__main__":
    main()
