#!/usr/bin/env python3
"""Expand Vintage Story blocktype JSON5 files into concrete block codes.

Usage: python3 -I vsexpand.py <vs_install_dir> <out_file> [entity_classes.json]

Logic mirrors VS RegistryObjectType variant resolution (approximation):
- domain: assets/game, assets/survival, assets/creative are all *origins* of the
  single "game" domain (no assets/<domain>/ nesting), so every code is "game:...".
- variantgroups: each group gives states (states[] + loadFromProperties +
  loadFromPropertiesCombine values appended). Groups with combine "multiply"
  (default) are cartesian-multiplied in order; groups with combine "add" produce
  standalone variants "<code>-<state>"; "selectiveMultiply" (unused in 1.22.7)
  is treated as multiply.
- allowedVariants: keep only codes matching any pattern; skipVariants: drop codes
  matching any pattern. Patterns: '*' glob or '@regex', matched on the path
  (without domain), case-insensitive.
- blocktypes with enabled: false/0 are skipped.
"""
import json, os, re, sys, itertools


class P:
    def __init__(self, s):
        self.s, self.i = s, 0

    def ws(self):
        s = self.s
        while self.i < len(s):
            c = s[self.i]
            if c in ' \t\r\n﻿\xa0':
                self.i += 1
            elif s.startswith('//', self.i):
                j = s.find('\n', self.i)
                self.i = len(s) if j < 0 else j
            elif s.startswith('/*', self.i):
                j = s.find('*/', self.i + 2)
                self.i = len(s) if j < 0 else j + 2
            else:
                break

    def val(self):
        self.ws()
        c = self.s[self.i]
        if c == '{':
            return self.obj()
        if c == '[':
            return self.arr()
        if c in '"\'':
            return self.string()
        return self.bare()

    def string(self):
        q = self.s[self.i]
        self.i += 1
        out = []
        while True:
            c = self.s[self.i]
            if c == '\\':
                n = self.s[self.i + 1]
                out.append({'n': '\n', 't': '\t', 'r': '\r'}.get(n, n))
                if n == 'u':
                    out[-1] = chr(int(self.s[self.i + 2:self.i + 6], 16))
                    self.i += 4
                self.i += 2
                continue
            if c == q:
                self.i += 1
                return ''.join(out)
            out.append(c)
            self.i += 1

    def bare(self):
        m = re.compile(r'[A-Za-z0-9_$+\-.]+').match(self.s, self.i)
        if not m:
            raise ValueError('unexpected %r at %d' % (self.s[self.i:self.i + 20], self.i))
        self.i = m.end()
        t = m.group()
        if t == 'true': return True
        if t == 'false': return False
        if t == 'null': return None
        try:
            return int(t, 0) if not re.search(r'[.eE]', t) or t.startswith('0x') else float(t)
        except ValueError:
            try:
                return float(t)
            except ValueError:
                return t

    def key(self):
        self.ws()
        if self.s[self.i] in '"\'':
            return self.string()
        m = re.compile(r'[A-Za-z0-9_$\-.]+').match(self.s, self.i)
        self.i = m.end()
        return m.group()

    def obj(self):
        self.i += 1
        d = {}
        while True:
            self.ws()
            if self.s[self.i] == '}':
                self.i += 1
                return d
            k = self.key()
            self.ws()
            assert self.s[self.i] == ':', (self.s[self.i:self.i + 30], self.i)
            self.i += 1
            d[k] = self.val()
            self.ws()
            if self.s[self.i] == ',':
                self.i += 1

    def arr(self):
        self.i += 1
        a = []
        while True:
            self.ws()
            if self.s[self.i] == ']':
                self.i += 1
                return a
            a.append(self.val())
            self.ws()
            if self.s[self.i] == ',':
                self.i += 1


def load(path):
    with open(path, encoding='utf-8-sig') as f:
        return P(f.read()).val()


def ci(d, k, default=None):
    """case-insensitive key lookup (VS uses Newtonsoft, case-insensitive)."""
    for kk, v in d.items():
        if kk.lower() == k.lower():
            return v
    return default


def wildcard(pat):
    if pat.startswith('@'):
        return re.compile('^' + pat[1:] + '$', re.I)
    return re.compile('^' + '.*'.join(re.escape(x) for x in pat.split('*')) + '$', re.I)


def main(root, out):
    assets = os.path.join(root, 'assets')
    props_dirs = [os.path.join(assets, o, 'worldproperties') for o in ('game', 'survival', 'creative')]
    prop_cache = {}

    def prop(name):
        name = name.split(':')[-1]
        if name not in prop_cache:
            for d in props_dirs:
                p = os.path.join(d, name + '.json')
                if os.path.exists(p):
                    prop_cache[name] = [ci(v, 'code') for v in ci(load(p), 'variants')]
                    break
            else:
                print('WARN missing property', name, file=sys.stderr)
                prop_cache[name] = []
        return prop_cache[name]

    codes = set()
    entity = {}
    light = {}
    errors = 0
    nfiles = 0
    for origin in ('game', 'survival', 'creative'):
        bt = os.path.join(assets, origin, 'blocktypes')
        for dp, _, fns in os.walk(bt):
            for fn in fns:
                if not fn.endswith('.json'):
                    continue
                path = os.path.join(dp, fn)
                try:
                    data = load(path)
                except Exception as e:
                    print('PARSE FAIL', path, e, file=sys.stderr)
                    errors += 1
                    continue
                for bt_ in (data if isinstance(data, list) else [data]):
                    en = ci(bt_, 'enabled', True)
                    if en in (False, 0):
                        continue
                    nfiles += 1
                    base = ci(bt_, 'code')
                    groups = ci(bt_, 'variantgroups') or []
                    mul, add = [], []
                    for g in groups:
                        st = list(ci(g, 'states') or [])
                        if ci(g, 'loadFromProperties'):
                            st += prop(ci(g, 'loadFromProperties'))
                        for p in ci(g, 'loadFromPropertiesCombine') or []:
                            st += prop(p)
                        comb = (ci(g, 'combine') or 'multiply').lower()
                        (add if comb == 'add' else mul).append(st)
                    variants = []
                    if mul:
                        for combo in itertools.product(*mul):
                            variants.append(base + '-' + '-'.join(combo))
                    elif not add:
                        variants.append(base)
                    for st in add:
                        for s in st:
                            variants.append(base + '-' + s)
                    allow = [wildcard(p) for p in ci(bt_, 'allowedVariants') or []]
                    skip = [wildcard(p) for p in ci(bt_, 'skipVariants') or []]
                    lh = ci(bt_, 'lightHsv')
                    lhbt = [(wildcard(k), v) for k, v in (ci(bt_, 'lightHsvByType') or {}).items()]
                    ec = ci(bt_, 'entityClass')
                    ecbt = [(wildcard(k), v) for k, v in (ci(bt_, 'entityClassByType') or {}).items()]
                    for v in variants:
                        if allow and not any(r.match(v) for r in allow):
                            continue
                        if any(r.match(v) for r in skip):
                            continue
                        codes.add('game:' + v.lower())
                        e = ec
                        for r, val in ecbt:
                            if r.match(v):
                                e = val
                                break
                        if e:
                            entity['game:' + v.lower()] = e
                        hsv = lh
                        for r, val in lhbt:
                            if r.match(v):
                                hsv = val
                                break
                        if hsv and len(hsv) == 3 and hsv[2] > 0:
                            light['game:' + v.lower()] = hsv
    codes.add('game:air')     # hard-coded block id 0 in VS (no blocktype json)
    codes.add('game:mantle')  # hard-coded in VSEssentials (bedrock equivalent, no json)
    if len(sys.argv) > 3:
        if sys.argv[3].endswith('.tsv'):
            with open(sys.argv[3], 'w') as f:
                f.write('# code VS\tclasse de block entity (entityClass)\n')
                for code in sorted(entity):
                    f.write('%s\t%s\n' % (code, entity[code]))
        else:
            json.dump(entity, open(sys.argv[3], 'w'), indent=0, sort_keys=True)
    if len(sys.argv) > 4:
        with open(sys.argv[4], 'w') as f:
            f.write('# code VS\tteinte\tsaturation\tintensité (lightHsv)\n')
            for code in sorted(light):
                f.write('%s\t%d\t%d\t%d\n' % (code, *light[code]))
        print('light sources', len(light), file=sys.stderr)
    with open(out, 'w') as f:
        f.write('\n'.join(sorted(codes)) + '\n')
    print('blocktypes', nfiles, 'codes', len(codes), 'parse errors', errors, file=sys.stderr)


if __name__ == '__main__':
    # Usage : vsexpand.py <dossier VS> <codes.txt> [entités.tsv|.json] [lumières.tsv]
    main(sys.argv[1], sys.argv[2])
