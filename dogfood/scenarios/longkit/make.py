#!/usr/bin/env python3
"""Generates the longkit scenario: 60 modules with business rules, one task each, plus a shared
CHANGELOG. Writes seed/ (stubs and tests) and tasks.tsv, then checks the tests: every test is
skipped on the seed, and every test passes with the reference implementations below (which are
not part of the seed, so agents never see them).

    python3 dogfood/scenarios/longkit/make.py
"""

import pathlib
import shutil
import subprocess
import sys
import tempfile
import textwrap

HERE = pathlib.Path(__file__).parent
PKG = "kit"

# module: (functions as "name(signature) -> type", docstring), tests (method bodies), reference
SPECS = {
    "semver": (
        [("compare", "a: str, b: str", "int",
          "Compare two semantic versions MAJOR.MINOR.PATCH[-PRERELEASE]: -1, 0, or 1. A prerelease sorts "
          "before its release; prerelease identifiers (dot-separated) compare numerically when both are "
          "digits, otherwise as text, and numeric ones sort before alphanumeric ones; if all shared "
          "identifiers are equal, the shorter list sorts first.")],
        """
        self.assertEqual(compare("1.0.0", "2.0.0"), -1)
        self.assertEqual(compare("1.10.0", "1.9.0"), 1)
        self.assertEqual(compare("1.2.3", "1.2.3"), 0)
        self.assertEqual(compare("1.0.0-alpha", "1.0.0"), -1)
        self.assertEqual(compare("1.0.0-alpha.1", "1.0.0-alpha"), 1)
        self.assertEqual(compare("1.0.0-alpha.beta", "1.0.0-alpha.1"), 1)
        self.assertEqual(compare("1.0.0-2", "1.0.0-10"), -1)
        """,
        """
        def compare(a, b):
            def parse(v):
                core, _, pre = v.partition("-")
                return tuple(int(x) for x in core.split(".")), pre.split(".") if pre else []
            (na, pa), (nb, pb) = parse(a), parse(b)
            if na != nb:
                return -1 if na < nb else 1
            if not pa or not pb:
                return 0 if pa == pb else (1 if not pa else -1)
            for x, y in zip(pa, pb):
                if x == y:
                    continue
                if x.isdigit() and y.isdigit():
                    return -1 if int(x) < int(y) else 1
                if x.isdigit() or y.isdigit():
                    return -1 if x.isdigit() else 1
                return -1 if x < y else 1
            return 0 if len(pa) == len(pb) else (-1 if len(pa) < len(pb) else 1)
        """,
    ),
    "csvline": (
        [("parse", "line: str", "list",
          "Split one CSV line on commas. A field may be wrapped in double quotes, inside which commas are "
          "kept and a doubled quote stands for one quote. Whitespace is kept. An empty line is one empty field.")],
        '''
        self.assertEqual(parse("a,b,c"), ["a", "b", "c"])
        self.assertEqual(parse('"a,b",c'), ["a,b", "c"])
        self.assertEqual(parse('"say ""hi""",x'), ['say "hi"', "x"])
        self.assertEqual(parse("a,,b"), ["a", "", "b"])
        self.assertEqual(parse(""), [""])
        self.assertEqual(parse(" a , b "), [" a ", " b "])
        ''',
        """
        def parse(line):
            fields, cur, quoted, i = [], "", False, 0
            while i < len(line):
                c = line[i]
                if quoted:
                    if c == '"' and line[i + 1:i + 2] == '"':
                        cur += '"'
                        i += 1
                    elif c == '"':
                        quoted = False
                    else:
                        cur += c
                elif c == '"':
                    quoted = True
                elif c == ",":
                    fields.append(cur)
                    cur = ""
                else:
                    cur += c
                i += 1
            return fields + [cur]
        """,
    ),
    "luhn": (
        [("valid", "number: str", "bool",
          "Whether a number passes the Luhn check. Spaces are ignored; any other non-digit, or fewer than "
          "two digits, makes it invalid."),
         ("check_digit", "partial: str", "str",
          "The digit that, appended to `partial`, makes it pass the Luhn check.")],
        """
        self.assertTrue(valid("4539 1488 0343 6467"))
        self.assertFalse(valid("8273 1232 7352 0569"))
        self.assertFalse(valid("0"))
        self.assertFalse(valid("059a"))
        self.assertTrue(valid("79927398713"))
        self.assertEqual(check_digit("7992739871"), "3")
        """,
        """
        def _sum(digits):
            total = 0
            for i, d in enumerate(reversed(digits)):
                d = int(d)
                if i % 2 == 1:
                    d = d * 2 - 9 if d > 4 else d * 2
                total += d
            return total
        def valid(number):
            digits = number.replace(" ", "")
            return len(digits) > 1 and digits.isdigit() and _sum(digits) % 10 == 0
        def check_digit(partial):
            return str((10 - _sum(partial + "0") % 10) % 10)
        """,
    ),
    "roman": (
        [("to_roman", "n: int", "str", "Roman numeral for 1 <= n <= 3999; ValueError otherwise."),
         ("from_roman", "s: str", "int", "Value of a numeral in canonical form (as to_roman writes it); ValueError otherwise.")],
        """
        self.assertEqual(to_roman(1994), "MCMXCIV")
        self.assertEqual(to_roman(3999), "MMMCMXCIX")
        self.assertEqual(from_roman("MCMXCIV"), 1994)
        self.assertEqual(from_roman("IV"), 4)
        for bad in ("IIII", "", "IC", "ABC"):
            with self.assertRaises(ValueError):
                from_roman(bad)
        for bad in (0, 4000):
            with self.assertRaises(ValueError):
                to_roman(bad)
        """,
        """
        TABLE = [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"),
                 (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")]
        def to_roman(n):
            if not 1 <= n <= 3999:
                raise ValueError(n)
            out = ""
            for v, s in TABLE:
                while n >= v:
                    out += s
                    n -= v
            return out
        def from_roman(s):
            n, i = 0, 0
            for v, sym in TABLE:
                while s.startswith(sym, i):
                    n += v
                    i += len(sym)
            if not s or i != len(s) or not 1 <= n <= 3999 or to_roman(n) != s:
                raise ValueError(s)
            return n
        """,
    ),
    "duration": (
        [("parse", "text: str", "int",
          "Seconds in a duration like '1d2h3m4s': units d, h, m, s, in that order, each at most once, no "
          "spaces, at least one. ValueError otherwise."),
         ("format", "seconds: int", "str", "The shortest such text for `seconds` ('0s' for 0).")],
        """
        self.assertEqual(parse("1h30m"), 5400)
        self.assertEqual(parse("45s"), 45)
        self.assertEqual(parse("2d"), 172800)
        for bad in ("", "1x", "30m1h", "1h1h", "1 h"):
            with self.assertRaises(ValueError):
                parse(bad)
        self.assertEqual(format(5400), "1h30m")
        self.assertEqual(format(0), "0s")
        self.assertEqual(format(90061), "1d1h1m1s")
        """,
        """
        import re
        UNITS = [("d", 86400), ("h", 3600), ("m", 60), ("s", 1)]
        def parse(text):
            m = re.fullmatch(r"(?:(\\d+)d)?(?:(\\d+)h)?(?:(\\d+)m)?(?:(\\d+)s)?", text)
            if not text or not m:
                raise ValueError(text)
            return sum(int(g) * f for g, (_, f) in zip(m.groups(), UNITS) if g)
        def format(seconds):
            out = ""
            for u, f in UNITS:
                q, seconds = divmod(seconds, f)
                if q:
                    out += f"{q}{u}"
            return out or "0s"
        """,
    ),
    "ranges": (
        [("expand", "spec: str", "list",
          "Numbers in a spec like '1-3,5,7-9', sorted and without duplicates; '' is empty. A range whose "
          "start is above its end is a ValueError."),
         ("compress", "numbers: list", "str", "The shortest spec for a list of integers (any order, duplicates allowed).")],
        """
        self.assertEqual(expand("1-3,5,7-9"), [1, 2, 3, 5, 7, 8, 9])
        self.assertEqual(expand(""), [])
        self.assertEqual(expand("3,1-2,2"), [1, 2, 3])
        with self.assertRaises(ValueError):
            expand("5-3")
        self.assertEqual(compress([1, 2, 3, 5, 7, 8, 9]), "1-3,5,7-9")
        self.assertEqual(compress([]), "")
        self.assertEqual(compress([4, 2, 3, 3]), "2-4")
        self.assertEqual(compress([1, 2]), "1-2")
        """,
        """
        def expand(spec):
            out = set()
            for part in filter(None, spec.split(",")):
                a, _, b = part.partition("-")
                a, b = int(a), int(b or a)
                if a > b:
                    raise ValueError(part)
                out.update(range(a, b + 1))
            return sorted(out)
        def compress(numbers):
            parts, ns = [], sorted(set(numbers))
            i = 0
            while i < len(ns):
                j = i
                while j + 1 < len(ns) and ns[j + 1] == ns[j] + 1:
                    j += 1
                parts.append(str(ns[i]) if i == j else f"{ns[i]}-{ns[j]}")
                i = j + 1
            return ",".join(parts)
        """,
    ),
    "edits": (
        [("distance", "a: str, b: str", "int", "Levenshtein distance (insertions, deletions, substitutions)."),
         ("closest", "word: str, candidates: list", "object",
          "The candidate nearest to `word` by distance, the earliest one on ties; None if there are none.")],
        """
        self.assertEqual(distance("kitten", "sitting"), 3)
        self.assertEqual(distance("", "abc"), 3)
        self.assertEqual(distance("flaw", "lawn"), 2)
        self.assertEqual(closest("appel", ["ape", "apple", "apply"]), "ape")
        self.assertEqual(closest("aple", ["maple", "apple"]), "maple")
        self.assertIsNone(closest("x", []))
        """,
        """
        def distance(a, b):
            prev = list(range(len(b) + 1))
            for i, ca in enumerate(a, 1):
                cur = [i]
                for j, cb in enumerate(b, 1):
                    cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (ca != cb)))
                prev = cur
            return prev[-1]
        def closest(word, candidates):
            best = None
            for c in candidates:
                if best is None or distance(word, c) < distance(word, best):
                    best = c
            return best
        """,
    ),
    "table": (
        [("render", "headers: list, rows: list", "str",
          "A Markdown table: every column padded (left-aligned) to its widest cell, cells converted with "
          "str, a separator row of dashes as wide as each column, every line ending in a newline.")],
        """
        self.assertEqual(
            render(["name", "qty"], [["apple", 3], ["kiwi", 12]]),
            "| name  | qty |\\n| ----- | --- |\\n| apple | 3   |\\n| kiwi  | 12  |\\n",
        )
        self.assertEqual(render(["a"], []), "| a |\\n| - |\\n")
        """,
        """
        def render(headers, rows):
            cells = [[str(c) for c in r] for r in [headers] + list(rows)]
            widths = [max(len(r[i]) for r in cells) for i in range(len(headers))]
            line = lambda r: "| " + " | ".join(c.ljust(w) for c, w in zip(r, widths)) + " |\\n"
            return line(cells[0]) + line(["-" * w for w in widths]) + "".join(line(r) for r in cells[1:])
        """,
    ),
    "wrap": (
        [("wrap", "text: str, width: int", "list",
          "Greedy word wrap into lines of at most `width` characters, words separated by single spaces. "
          "A word longer than `width` is cut into pieces of `width` characters (the last may be shorter), "
          "each placed like a word.")],
        """
        self.assertEqual(wrap("the quick brown fox", 10), ["the quick", "brown fox"])
        self.assertEqual(wrap("", 5), [])
        self.assertEqual(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"])
        self.assertEqual(wrap("a bcdefgh", 4), ["a", "bcde", "fgh"])
        self.assertEqual(wrap("ab cdefgh i", 4), ["ab", "cdef", "gh i"])
        """,
        """
        def wrap(text, width):
            pieces = [w[i:i + width] for w in text.split() for i in range(0, len(w), width)]
            lines = []
            for p in pieces:
                if lines and len(lines[-1]) + 1 + len(p) <= width:
                    lines[-1] += " " + p
                else:
                    lines.append(p)
            return lines
        """,
    ),
    "slug": (
        [("slugify", "text: str", "str",
          "Lowercase ASCII slug: accents removed (NFKD), every run of other characters replaced by one '-', "
          "no '-' at either end.")],
        """
        self.assertEqual(slugify("Héllo, Wörld!"), "hello-world")
        self.assertEqual(slugify("  --A  B--  "), "a-b")
        self.assertEqual(slugify(""), "")
        self.assertEqual(slugify("C++ & Rust"), "c-rust")
        """,
        """
        import re, unicodedata
        def slugify(text):
            ascii_text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
            return re.sub(r"[^a-z0-9]+", "-", ascii_text.lower()).strip("-")
        """,
    ),
    "intervals": (
        [("merge", "intervals: list", "list",
          "Merge [start, end] intervals, sorted by start; touching intervals merge. ValueError if a start "
          "is above its end.")],
        """
        self.assertEqual(merge([[1, 3], [2, 6], [8, 10], [15, 18]]), [[1, 6], [8, 10], [15, 18]])
        self.assertEqual(merge([[1, 4], [4, 5]]), [[1, 5]])
        self.assertEqual(merge([]), [])
        self.assertEqual(merge([[5, 6], [1, 2]]), [[1, 2], [5, 6]])
        with self.assertRaises(ValueError):
            merge([[3, 1]])
        """,
        """
        def merge(intervals):
            out = []
            for s, e in sorted(intervals):
                if s > e:
                    raise ValueError((s, e))
                if out and s <= out[-1][1]:
                    out[-1][1] = max(out[-1][1], e)
                else:
                    out.append([s, e])
            return out
        """,
    ),
    "bizdays": (
        [("add_business_days", "date: str, n: int", "str",
          "The ISO date (YYYY-MM-DD) `n` business days (Monday to Friday) after `date`, or before it if `n` "
          "is negative; n = 0 returns `date` unchanged. Counting starts from the next day, so a weekend "
          "start works.")],
        """
        self.assertEqual(add_business_days("2024-01-05", 1), "2024-01-08")
        self.assertEqual(add_business_days("2024-01-08", -1), "2024-01-05")
        self.assertEqual(add_business_days("2024-01-01", 10), "2024-01-15")
        self.assertEqual(add_business_days("2024-01-06", 1), "2024-01-08")
        self.assertEqual(add_business_days("2024-01-03", 0), "2024-01-03")
        self.assertEqual(add_business_days("2024-02-28", 2), "2024-03-01")
        """,
        """
        import datetime
        def add_business_days(date, n):
            d = datetime.date.fromisoformat(date)
            step = 1 if n >= 0 else -1
            left = abs(n)
            while left:
                d += datetime.timedelta(days=step)
                if d.weekday() < 5:
                    left -= 1
            return d.isoformat()
        """,
    ),
    "isbn": (
        [("valid", "isbn: str", "bool",
          "Whether an ISBN-10 (last character may be X) or ISBN-13 is valid; hyphens are ignored."),
         ("to_isbn13", "isbn10: str", "str", "The ISBN-13 (digits only) for a valid ISBN-10; ValueError otherwise.")],
        """
        self.assertTrue(valid("0-306-40615-2"))
        self.assertTrue(valid("978-0-306-40615-7"))
        self.assertFalse(valid("0-306-40615-3"))
        self.assertTrue(valid("080442957X"))
        self.assertFalse(valid("123"))
        self.assertEqual(to_isbn13("0-306-40615-2"), "9780306406157")
        with self.assertRaises(ValueError):
            to_isbn13("0-306-40615-3")
        """,
        """
        def valid(isbn):
            s = isbn.replace("-", "")
            if len(s) == 10 and s[:9].isdigit() and (s[9].isdigit() or s[9] == "X"):
                vals = [int(c) for c in s[:9]] + [10 if s[9] == "X" else int(s[9])]
                return sum(v * w for v, w in zip(vals, range(10, 0, -1))) % 11 == 0
            if len(s) == 13 and s.isdigit():
                return sum(int(c) * (1 if i % 2 == 0 else 3) for i, c in enumerate(s)) % 10 == 0
            return False
        def to_isbn13(isbn10):
            s = isbn10.replace("-", "")
            if len(s) != 10 or not valid(s):
                raise ValueError(isbn10)
            body = "978" + s[:9]
            check = (10 - sum(int(c) * (1 if i % 2 == 0 else 3) for i, c in enumerate(body)) % 10) % 10
            return body + str(check)
        """,
    ),
    "rle": (
        [("encode", "text: str", "str", "Run-length encoding: every run as <count><char>, e.g. 'aaab' -> '3a1b'."),
         ("decode", "data: str", "str", "The inverse of encode; ValueError if a count or a character is missing.")],
        """
        self.assertEqual(encode("aaabccdddd"), "3a1b2c4d")
        self.assertEqual(encode(""), "")
        self.assertEqual(encode("a" * 12), "12a")
        self.assertEqual(decode("3a1b2c4d"), "aaabccdddd")
        self.assertEqual(decode("12a"), "a" * 12)
        for bad in ("3", "a"):
            with self.assertRaises(ValueError):
                decode(bad)
        """,
        """
        import itertools, re
        def encode(text):
            return "".join(f"{len(list(g))}{c}" for c, g in itertools.groupby(text))
        def decode(data):
            if not re.fullmatch(r"(\\d+\\D)*", data):
                raise ValueError(data)
            return "".join(c * int(n) for n, c in re.findall(r"(\\d+)(\\D)", data))
        """,
    ),
    "spiral": (
        [("spiral", "matrix: list", "list", "The elements of a rectangular matrix in clockwise spiral order from the top left.")],
        """
        self.assertEqual(spiral([[1, 2, 3], [4, 5, 6], [7, 8, 9]]), [1, 2, 3, 6, 9, 8, 7, 4, 5])
        self.assertEqual(spiral([]), [])
        self.assertEqual(spiral([[1, 2, 3, 4]]), [1, 2, 3, 4])
        self.assertEqual(spiral([[1], [2], [3]]), [1, 2, 3])
        self.assertEqual(spiral([[1, 2], [3, 4], [5, 6]]), [1, 2, 4, 6, 5, 3])
        """,
        """
        def spiral(matrix):
            m, out = [list(r) for r in matrix], []
            while m:
                out += m.pop(0)
                m = [list(r) for r in zip(*m)][::-1]
            return out
        """,
    ),
    "jsonpath": (
        [("get", "obj: object, path: str", "object",
          "The value at a path like 'a.b[2].c' (keys separated by dots, list indexes in brackets, a path may "
          "start with an index); None if any step is missing.")],
        """
        self.assertEqual(get({"a": {"b": [0, 1, {"c": 5}]}}, "a.b[2].c"), 5)
        self.assertEqual(get({"a": [1, 2]}, "a[1]"), 2)
        self.assertIsNone(get({"a": 1}, "a.b"))
        self.assertIsNone(get({"a": [1]}, "a[5]"))
        self.assertEqual(get({"x": {"y": 2}}, "x.y"), 2)
        self.assertEqual(get([[1, 2], [3]], "[1][0]"), 3)
        """,
        """
        import re
        def get(obj, path):
            for key, index in re.findall(r"([^.\\[\\]]+)|\\[(\\d+)\\]", path):
                if key:
                    if not isinstance(obj, dict) or key not in obj:
                        return None
                    obj = obj[key]
                else:
                    if not isinstance(obj, list) or int(index) >= len(obj):
                        return None
                    obj = obj[int(index)]
            return obj
        """,
    ),
    "temperature": (
        [("convert", "value: float, src: str, dst: str", "float",
          "Convert between 'C', 'F', and 'K', rounded to 2 decimals. ValueError for another unit or a "
          "temperature below absolute zero.")],
        """
        self.assertEqual(convert(100, "C", "F"), 212.0)
        self.assertEqual(convert(32, "F", "C"), 0.0)
        self.assertEqual(convert(0, "K", "C"), -273.15)
        self.assertEqual(convert(-40, "C", "F"), -40.0)
        with self.assertRaises(ValueError):
            convert(-1, "K", "C")
        with self.assertRaises(ValueError):
            convert(1, "X", "C")
        """,
        """
        def convert(value, src, dst):
            to_k = {"C": lambda v: v + 273.15, "F": lambda v: (v - 32) * 5 / 9 + 273.15, "K": lambda v: v}
            from_k = {"C": lambda k: k - 273.15, "F": lambda k: (k - 273.15) * 9 / 5 + 32, "K": lambda k: k}
            if src not in to_k or dst not in from_k:
                raise ValueError((src, dst))
            k = to_k[src](value)
            if k < -1e-9:
                raise ValueError(value)
            return round(from_k[dst](k), 2)
        """,
    ),
    "bytesize": (
        [("format", "n: int", "str",
          "Below 1024: '<n> B'. Otherwise in KiB, MiB, GiB, or TiB (the largest unit with a value of at "
          "least 1) with one decimal, a trailing '.0' dropped: '1.5 KiB', '1 MiB'."),
         ("parse", "text: str", "int", "The inverse: '<number> <unit>' with unit B, KiB, MiB, GiB, or TiB; ValueError otherwise.")],
        """
        self.assertEqual(format(0), "0 B")
        self.assertEqual(format(1023), "1023 B")
        self.assertEqual(format(1536), "1.5 KiB")
        self.assertEqual(format(1048576), "1 MiB")
        self.assertEqual(format(1100), "1.1 KiB")
        self.assertEqual(parse("1.5 KiB"), 1536)
        self.assertEqual(parse("2 MiB"), 2097152)
        self.assertEqual(parse("10 B"), 10)
        with self.assertRaises(ValueError):
            parse("1 XB")
        """,
        """
        UNITS = ["B", "KiB", "MiB", "GiB", "TiB"]
        def format(n):
            if n < 1024:
                return f"{n} B"
            i = 0
            v = float(n)
            while v >= 1024 and i < len(UNITS) - 1:
                v /= 1024
                i += 1
            text = f"{v:.1f}"
            if text.endswith(".0"):
                text = text[:-2]
            return f"{text} {UNITS[i]}"
        def parse(text):
            number, _, unit = text.partition(" ")
            if unit not in UNITS:
                raise ValueError(text)
            return int(round(float(number) * 1024 ** UNITS.index(unit)))
        """,
    ),
    "query": (
        [("parse", "qs: str", "dict",
          "A query string as name -> list of values, in order: percent-decoding, '+' as space, a name "
          "without '=' has the value ''; '' gives {}."),
         ("build", "params: dict", "str", "The inverse: names sorted, each value in order, spaces as '+', other reserved characters percent-encoded.")],
        """
        self.assertEqual(parse("a=1&b=2&a=3"), {"a": ["1", "3"], "b": ["2"]})
        self.assertEqual(parse("q=hello+world%21"), {"q": ["hello world!"]})
        self.assertEqual(parse("flag"), {"flag": [""]})
        self.assertEqual(parse(""), {})
        self.assertEqual(build({"b": ["2"], "a": ["1", "x y"]}), "a=1&a=x+y&b=2")
        self.assertEqual(build({"k": ["a&b"]}), "k=a%26b")
        """,
        """
        from urllib.parse import parse_qsl, quote_plus
        def parse(qs):
            out = {}
            for k, v in parse_qsl(qs, keep_blank_values=True):
                out.setdefault(k, []).append(v)
            return out
        def build(params):
            return "&".join(f"{quote_plus(k)}={quote_plus(v)}" for k in sorted(params) for v in params[k])
        """,
    ),
    "reorder": (
        [("reorder", "stock: dict, sales_per_day: dict, lead_days: int, safety_days: int", "dict",
          "Quantities to order per item so that stock covers (lead_days + safety_days) of sales: "
          "ceil(sales_per_day * (lead_days + safety_days)) - stock, items missing from stock count as 0, "
          "items needing nothing left out.")],
        """
        self.assertEqual(reorder({"a": 10, "b": 100}, {"a": 3, "b": 1.5}, 5, 2), {"a": 11})
        self.assertEqual(reorder({}, {"c": 0.4}, 2, 1), {"c": 2})
        self.assertEqual(reorder({"d": 5}, {"d": 1}, 3, 2), {})
        self.assertEqual(reorder({"e": 0}, {}, 3, 2), {})
        """,
        """
        import math
        def reorder(stock, sales_per_day, lead_days, safety_days):
            out = {}
            for item, rate in sales_per_day.items():
                need = math.ceil(rate * (lead_days + safety_days)) - stock.get(item, 0)
                if need > 0:
                    out[item] = need
            return out
        """,
    ),
}

from more_specs import MORE  # noqa: E402  (modules 21 to 60)

SPECS.update(MORE)


def stub_module(functions) -> str:
    out = f"from {PKG}._stub import stub\n"
    for name, sig, ret, doc in functions:
        out += f'\n\n@stub\ndef {name}({sig}) -> {ret}:\n    """{doc}"""\n    raise NotImplementedError\n'
    return out


def test_module(module, functions, body) -> str:
    names = ", ".join(f[0] for f in functions)
    return (
        f"import unittest\n\nfrom {PKG}._stub import is_stub\nfrom {PKG}.{module} import {names}\n\n\n"
        f"class Test(unittest.TestCase):\n    def setUp(self):\n"
        f"        if any(is_stub(f) for f in ({names},)):\n"
        f'            self.skipTest("not implemented yet")\n\n'
        f"    def test_{module}(self):\n" + textwrap.indent(textwrap.dedent(body).strip("\n"), "        ") + "\n"
    )


def run_tests(directory) -> str:
    result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", "tests"], cwd=directory,
                            capture_output=True, text=True)
    return result.stderr.strip().splitlines()[-1] if result.stderr.strip() else "?"


def main() -> None:
    seed = HERE / "seed"
    shutil.rmtree(seed, ignore_errors=True)
    (seed / PKG).mkdir(parents=True)
    (seed / "tests").mkdir()
    (seed / PKG / "__init__.py").write_text("")
    (seed / PKG / "_stub.py").write_text((HERE.parent / "pipeline/seed/pipeline/_stub.py").read_text())
    (seed / ".gitignore").write_text("__pycache__/\n*.pyc\n")
    (seed / "CHANGELOG.md").write_text("# Changelog\n\n## Unreleased\n\n")
    (seed / "README.md").write_text(
        f"# {PKG}\n\nSixty small modules with business rules. Run the tests with "
        "`python3 -m unittest discover -s tests`; a test is skipped while its module is a stub.\n")
    tasks = []
    for module, (functions, body, _ref) in SPECS.items():
        (seed / PKG / f"{module}.py").write_text(stub_module(functions))
        (seed / "tests" / f"test_{module}.py").write_text(test_module(module, functions, body))
        names = ", ".join(f[0] for f in functions)
        tasks.append(f"Implement {PKG}.{module}\tImplement {names} in {PKG}/{module}.py so that "
                     f"tests/test_{module}.py passes, and add a CHANGELOG line.\t{PKG}/{module}.py")
    (HERE / "tasks.tsv").write_text("\n".join(tasks) + "\n")

    seed_result = run_tests(seed)
    with tempfile.TemporaryDirectory() as tmp:
        shutil.copytree(seed, tmp, dirs_exist_ok=True)
        for module, (_f, _b, ref) in SPECS.items():
            (pathlib.Path(tmp) / PKG / f"{module}.py").write_text(textwrap.dedent(ref).lstrip())
        ref_result = run_tests(tmp)
    shutil.rmtree(seed / "tests" / "__pycache__", ignore_errors=True)
    for cache in seed.rglob("__pycache__"):
        shutil.rmtree(cache)
    print(f"{len(SPECS)} tasks; seed: {seed_result}; reference: {ref_result}")
    if not (seed_result.startswith("OK (skipped=") and ref_result == "OK"):
        sys.exit(1)


if __name__ == "__main__":
    main()
