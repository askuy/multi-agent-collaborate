"""Modules 21 to 60 of the longkit scenario, in the same shape as SPECS in make.py:
module -> (functions [(name, signature, return type, docstring)], test body, reference)."""

MORE = {
    "caesar": (
        [("shift", "text: str, n: int", "str",
          "Rotate every ASCII letter by n places (n may be negative or above 26), keeping its case; "
          "other characters are unchanged.")],
        """
        self.assertEqual(shift("abc", 1), "bcd")
        self.assertEqual(shift("xyz", 3), "abc")
        self.assertEqual(shift("Hello, World!", 13), "Uryyb, Jbeyq!")
        self.assertEqual(shift("abc", -1), "zab")
        self.assertEqual(shift("abc", 27), "bcd")
        """,
        """
        def shift(text, n):
            out = []
            for c in text:
                if "a" <= c <= "z":
                    out.append(chr((ord(c) - 97 + n) % 26 + 97))
                elif "A" <= c <= "Z":
                    out.append(chr((ord(c) - 65 + n) % 26 + 65))
                else:
                    out.append(c)
            return "".join(out)
        """,
    ),
    "anagram": (
        [("groups", "words: list", "list",
          "Group words that are anagrams of each other (ignoring case): each group keeps input order, "
          "groups are ordered by their first word's position; a word with no anagram is a group of one.")],
        """
        self.assertEqual(
            groups(["tea", "eat", "tan", "ate", "nat", "bat"]),
            [["tea", "eat", "ate"], ["tan", "nat"], ["bat"]],
        )
        self.assertEqual(groups([]), [])
        self.assertEqual(groups(["Listen", "silent"]), [["Listen", "silent"]])
        """,
        """
        def groups(words):
            out = {}
            for w in words:
                out.setdefault("".join(sorted(w.lower())), []).append(w)
            return list(out.values())
        """,
    ),
    "palindrome": (
        [("longest", "text: str", "str",
          "The longest palindromic substring (case-sensitive), the earliest one on ties; '' for ''.")],
        """
        self.assertEqual(longest("babad"), "bab")
        self.assertEqual(longest("cbbd"), "bb")
        self.assertEqual(longest(""), "")
        self.assertEqual(longest("abc"), "a")
        self.assertEqual(longest("forgeeksskeegfor"), "geeksskeeg")
        """,
        """
        def longest(text):
            best = ""
            for i in range(len(text)):
                for j in range(len(text), i, -1):
                    s = text[i:j]
                    if len(s) > len(best) and s == s[::-1]:
                        best = s
                        break
            return best
        """,
    ),
    "primes": (
        [("factorize", "n: int", "list", "Prime factors of n >= 2 in ascending order, with multiplicity; ValueError for n < 2."),
         ("primes_up_to", "n: int", "list", "All primes <= n in ascending order.")],
        """
        self.assertEqual(factorize(60), [2, 2, 3, 5])
        self.assertEqual(factorize(97), [97])
        with self.assertRaises(ValueError):
            factorize(1)
        self.assertEqual(primes_up_to(20), [2, 3, 5, 7, 11, 13, 17, 19])
        self.assertEqual(primes_up_to(1), [])
        """,
        """
        def factorize(n):
            if n < 2:
                raise ValueError(n)
            out, d = [], 2
            while d * d <= n:
                while n % d == 0:
                    out.append(d)
                    n //= d
                d += 1
            return out + ([n] if n > 1 else [])
        def primes_up_to(n):
            return [k for k in range(2, n + 1) if all(k % d for d in range(2, int(k ** 0.5) + 1))]
        """,
    ),
    "bowling": (
        [("score", "rolls: list", "int",
          "Total score of a complete ten-pin bowling game given every roll (strikes and spares get their "
          "bonus rolls; the tenth frame's bonus rolls only count as bonus).")],
        """
        self.assertEqual(score([0] * 20), 0)
        self.assertEqual(score([1] * 20), 20)
        self.assertEqual(score([10] * 12), 300)
        self.assertEqual(score([5, 5] * 10 + [5]), 150)
        self.assertEqual(score([10, 7, 3, 9, 0, 10, 0, 8, 8, 2, 0, 6, 10, 10, 10, 8, 1]), 167)
        """,
        """
        def score(rolls):
            total, i = 0, 0
            for _ in range(10):
                if rolls[i] == 10:
                    total += 10 + rolls[i + 1] + rolls[i + 2]
                    i += 1
                elif rolls[i] + rolls[i + 1] == 10:
                    total += 10 + rolls[i + 2]
                    i += 2
                else:
                    total += rolls[i] + rolls[i + 1]
                    i += 2
            return total
        """,
    ),
    "password": (
        [("strength", "password: str", "int",
          "One point each for: length >= 8, a lowercase letter, an uppercase letter, a digit, any other "
          "character. A common password ('password', '123456', 'qwerty', compared ignoring case) scores 0.")],
        """
        self.assertEqual(strength(""), 0)
        self.assertEqual(strength("abc"), 1)
        self.assertEqual(strength("Abcdefg1!"), 5)
        self.assertEqual(strength("PASSWORD"), 0)
        self.assertEqual(strength("abcdefgh"), 2)
        """,
        """
        def strength(password):
            if password.lower() in {"password", "123456", "qwerty"}:
                return 0
            return sum([
                len(password) >= 8,
                any(c.islower() for c in password),
                any(c.isupper() for c in password),
                any(c.isdigit() for c in password),
                any(not c.isalnum() for c in password),
            ])
        """,
    ),
    "case": (
        [("to_snake", "name: str", "str",
          "camelCase or PascalCase to snake_case; a run of capitals is one word ('parseHTTPResponse' -> "
          "'parse_http_response')."),
         ("to_camel", "name: str", "str", "snake_case to camelCase ('hello_world' -> 'helloWorld').")],
        """
        self.assertEqual(to_snake("parseHTTPResponse"), "parse_http_response")
        self.assertEqual(to_snake("simple"), "simple")
        self.assertEqual(to_snake("HelloWorld"), "hello_world")
        self.assertEqual(to_camel("parse_http_response"), "parseHttpResponse")
        self.assertEqual(to_camel("hello_world"), "helloWorld")
        self.assertEqual(to_camel("x"), "x")
        """,
        r"""
        import re
        def to_snake(name):
            s = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1_\2", name)
            return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", s).lower()
        def to_camel(name):
            parts = [p for p in name.split("_") if p]
            return parts[0].lower() + "".join(p.capitalize() for p in parts[1:])
        """,
    ),
    "phone": (
        [("format", "number: str", "str",
          "A North American number as '(NXX) NXX-XXXX'. Spaces, dots, dashes, parentheses, and a leading '+' "
          "are ignored; a leading country code 1 is dropped; then exactly 10 digits must remain, and the "
          "area code and exchange must start with 2-9. ValueError otherwise.")],
        """
        self.assertEqual(format("613-995-0253"), "(613) 995-0253")
        self.assertEqual(format("+1 (613) 995-0253"), "(613) 995-0253")
        self.assertEqual(format("613.995.0253"), "(613) 995-0253")
        for bad in ("123-456-7890", "613-095-0253", "22-995-0253", "613-995-025a"):
            with self.assertRaises(ValueError):
                format(bad)
        """,
        """
        def format(number):
            s = number.lstrip("+")
            for c in " .-()":
                s = s.replace(c, "")
            if len(s) == 11 and s.startswith("1"):
                s = s[1:]
            if len(s) != 10 or not s.isdigit() or s[0] in "01" or s[3] in "01":
                raise ValueError(number)
            return f"({s[:3]}) {s[3:6]}-{s[6:]}"
        """,
    ),
    "matmul": (
        [("multiply", "a: list, b: list", "list", "Matrix product of two lists of rows; ValueError if the shapes do not fit.")],
        """
        self.assertEqual(multiply([[1, 2], [3, 4]], [[5, 6], [7, 8]]), [[19, 22], [43, 50]])
        self.assertEqual(multiply([[1, 2, 3]], [[1], [2], [3]]), [[14]])
        with self.assertRaises(ValueError):
            multiply([[1, 2]], [[1, 2]])
        """,
        """
        def multiply(a, b):
            if not a or not b or any(len(r) != len(b) for r in a):
                raise ValueError("shapes")
            return [[sum(x * y for x, y in zip(r, col)) for col in zip(*b)] for r in a]
        """,
    ),
    "rpn": (
        [("evaluate", "expr: str", "int",
          "Evaluate a reverse Polish expression of integers and + - * / separated by spaces; / truncates "
          "toward zero. ValueError for a malformed expression, ZeroDivisionError for division by zero.")],
        """
        self.assertEqual(evaluate("3 4 + 2 *"), 14)
        self.assertEqual(evaluate("7 -2 /"), -3)
        self.assertEqual(evaluate("5"), 5)
        for bad in ("1 +", "1 2", "1 x +", ""):
            with self.assertRaises(ValueError):
                evaluate(bad)
        with self.assertRaises(ZeroDivisionError):
            evaluate("1 0 /")
        """,
        """
        def evaluate(expr):
            stack = []
            for tok in expr.split():
                if tok in ("+", "-", "*", "/"):
                    if len(stack) < 2:
                        raise ValueError(expr)
                    b, a = stack.pop(), stack.pop()
                    if tok == "/":
                        if b == 0:
                            raise ZeroDivisionError(expr)
                        q = abs(a) // abs(b)
                        stack.append(q if (a >= 0) == (b >= 0) else -q)
                    else:
                        stack.append(a + b if tok == "+" else a - b if tok == "-" else a * b)
                else:
                    try:
                        stack.append(int(tok))
                    except ValueError:
                        raise ValueError(tok) from None
            if len(stack) != 1:
                raise ValueError(expr)
            return stack[0]
        """,
    ),
    "brackets": (
        [("balanced", "text: str", "bool", "Whether (), [], and {} are balanced and properly nested; other characters are ignored.")],
        """
        self.assertTrue(balanced("{[()]}"))
        self.assertFalse(balanced("([)]"))
        self.assertTrue(balanced(""))
        self.assertFalse(balanced("((("))
        self.assertFalse(balanced("a(b)c]"))
        """,
        """
        def balanced(text):
            pairs, stack = {")": "(", "]": "[", "}": "{"}, []
            for c in text:
                if c in "([{":
                    stack.append(c)
                elif c in pairs:
                    if not stack or stack.pop() != pairs[c]:
                        return False
            return not stack
        """,
    ),
    "wordfreq": (
        [("top", "text: str", "list", "placeholder")],
        "",
        "",
    ),
    "lcs": (
        [("length", "a: str, b: str", "int", "Length of the longest common subsequence."),
         ("sequence", "a: str, b: str", "str", "One longest common subsequence.")],
        """
        self.assertEqual(length("ABCBDAB", "BDCABA"), 4)
        self.assertEqual(length("", "a"), 0)
        s = sequence("ABCBDAB", "BDCABA")
        self.assertEqual(len(s), 4)
        for whole in ("ABCBDAB", "BDCABA"):
            rest = iter(whole)
            self.assertTrue(all(c in rest for c in s), s)
        self.assertEqual(sequence("abc", "xyz"), "")
        """,
        """
        def _table(a, b):
            t = [[0] * (len(b) + 1) for _ in range(len(a) + 1)]
            for i in range(len(a) - 1, -1, -1):
                for j in range(len(b) - 1, -1, -1):
                    t[i][j] = t[i + 1][j + 1] + 1 if a[i] == b[j] else max(t[i + 1][j], t[i][j + 1])
            return t
        def length(a, b):
            return _table(a, b)[0][0]
        def sequence(a, b):
            t, i, j, out = _table(a, b), 0, 0, ""
            while i < len(a) and j < len(b):
                if a[i] == b[j]:
                    out += a[i]
                    i += 1
                    j += 1
                elif t[i + 1][j] >= t[i][j + 1]:
                    i += 1
                else:
                    j += 1
            return out
        """,
    ),
    "baseconv": (
        [("to_base", "n: int, base: int", "str", "n in a base from 2 to 36 (digits 0-9 then a-z, a leading '-' if negative); ValueError for another base."),
         ("from_base", "text: str, base: int", "int", "The inverse (letters in either case); ValueError for an invalid digit or base.")],
        """
        self.assertEqual(to_base(255, 16), "ff")
        self.assertEqual(to_base(0, 2), "0")
        self.assertEqual(to_base(-10, 2), "-1010")
        self.assertEqual(from_base("ff", 16), 255)
        self.assertEqual(from_base("Z", 36), 35)
        with self.assertRaises(ValueError):
            from_base("2", 2)
        with self.assertRaises(ValueError):
            to_base(5, 1)
        """,
        """
        DIGITS = "0123456789abcdefghijklmnopqrstuvwxyz"
        def to_base(n, base):
            if not 2 <= base <= 36:
                raise ValueError(base)
            if n == 0:
                return "0"
            sign, n, out = "-" if n < 0 else "", abs(n), ""
            while n:
                n, r = divmod(n, base)
                out = DIGITS[r] + out
            return sign + out
        def from_base(text, base):
            if not 2 <= base <= 36:
                raise ValueError(base)
            return int(text, base)
        """,
    ),
    "iban": (
        [("valid", "iban: str", "bool",
          "Whether an IBAN is valid: spaces ignored, two letters, two digits, then letters and digits, "
          "15 to 34 characters in all, and the mod-97 check (rearranged, letters as 10-35) equals 1.")],
        """
        self.assertTrue(valid("GB82 WEST 1234 5698 7654 32"))
        self.assertFalse(valid("GB82 TEST 1234 5698 7654 32"))
        self.assertTrue(valid("DE89 3704 0044 0532 0130 00"))
        self.assertFalse(valid("XX"))
        self.assertFalse(valid("GB82 WEST 1234 5698 7654 3!"))
        """,
        """
        import re
        def valid(iban):
            s = iban.replace(" ", "").upper()
            if not re.fullmatch(r"[A-Z]{2}[0-9]{2}[A-Z0-9]{11,30}", s):
                return False
            digits = "".join(str(int(c, 36)) for c in s[4:] + s[:4])
            return int(digits) % 97 == 1
        """,
    ),
    "color": (
        [("hex_to_rgb", "text: str", "tuple", "'#rrggbb' or '#rgb' (either case) to an (r, g, b) tuple; ValueError otherwise."),
         ("rgb_to_hex", "r: int, g: int, b: int", "str", "Lowercase '#rrggbb'; ValueError for a value outside 0-255.")],
        """
        self.assertEqual(hex_to_rgb("#ff8800"), (255, 136, 0))
        self.assertEqual(hex_to_rgb("#F80"), (255, 136, 0))
        for bad in ("ff8800", "#ggg", "#12345"):
            with self.assertRaises(ValueError):
                hex_to_rgb(bad)
        self.assertEqual(rgb_to_hex(255, 136, 0), "#ff8800")
        with self.assertRaises(ValueError):
            rgb_to_hex(256, 0, 0)
        """,
        """
        import re
        def hex_to_rgb(text):
            if not re.fullmatch(r"#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})", text):
                raise ValueError(text)
            h = text[1:]
            if len(h) == 3:
                h = "".join(c * 2 for c in h)
            return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))
        def rgb_to_hex(r, g, b):
            if not all(0 <= v <= 255 for v in (r, g, b)):
                raise ValueError((r, g, b))
            return "#%02x%02x%02x" % (r, g, b)
        """,
    ),
    "clock12": (
        [("to_24h", "text: str", "str", "'1:05 PM' -> '13:05', '12:00 AM' -> '00:00'; ValueError for an invalid time."),
         ("to_12h", "text: str", "str", "'00:30' -> '12:30 AM', '23:59' -> '11:59 PM' (hour without a leading zero); ValueError for an invalid time.")],
        """
        self.assertEqual(to_24h("1:05 PM"), "13:05")
        self.assertEqual(to_24h("12:00 AM"), "00:00")
        self.assertEqual(to_24h("12:30 PM"), "12:30")
        self.assertEqual(to_12h("00:30"), "12:30 AM")
        self.assertEqual(to_12h("23:59"), "11:59 PM")
        self.assertEqual(to_12h("12:00"), "12:00 PM")
        with self.assertRaises(ValueError):
            to_24h("13:00 PM")
        with self.assertRaises(ValueError):
            to_12h("24:00")
        """,
        """
        import re
        def to_24h(text):
            m = re.fullmatch(r"(\\d{1,2}):(\\d{2}) (AM|PM)", text)
            if not m or not 1 <= int(m[1]) <= 12 or int(m[2]) > 59:
                raise ValueError(text)
            h = int(m[1]) % 12 + (12 if m[3] == "PM" else 0)
            return f"{h:02d}:{m[2]}"
        def to_12h(text):
            m = re.fullmatch(r"(\\d{2}):(\\d{2})", text)
            if not m or int(m[1]) > 23 or int(m[2]) > 59:
                raise ValueError(text)
            h = int(m[1])
            return f"{h % 12 or 12}:{m[2]} {'PM' if h >= 12 else 'AM'}"
        """,
    ),
    "months": (
        [("is_leap", "year: int", "bool", "Gregorian leap year."),
         ("days_in_month", "year: int, month: int", "int", "Days in the month; ValueError for a month outside 1-12.")],
        """
        self.assertTrue(is_leap(2000))
        self.assertFalse(is_leap(1900))
        self.assertTrue(is_leap(2024))
        self.assertFalse(is_leap(2023))
        self.assertEqual(days_in_month(2024, 2), 29)
        self.assertEqual(days_in_month(2023, 2), 28)
        self.assertEqual(days_in_month(2023, 4), 30)
        with self.assertRaises(ValueError):
            days_in_month(2023, 13)
        """,
        """
        def is_leap(year):
            return year % 4 == 0 and (year % 100 != 0 or year % 400 == 0)
        def days_in_month(year, month):
            if not 1 <= month <= 12:
                raise ValueError(month)
            if month == 2:
                return 29 if is_leap(year) else 28
            return 30 if month in (4, 6, 9, 11) else 31
        """,
    ),
    "age": (
        [("age", "birth: str, on: str", "int",
          "Completed years between two ISO dates; a 29 February birthday falls on 28 February in "
          "non-leap years. ValueError if `on` is before `birth`.")],
        """
        self.assertEqual(age("2000-05-15", "2024-05-14"), 23)
        self.assertEqual(age("2000-05-15", "2024-05-15"), 24)
        self.assertEqual(age("2004-02-29", "2023-02-28"), 19)
        self.assertEqual(age("2004-02-29", "2023-02-27"), 18)
        with self.assertRaises(ValueError):
            age("2024-01-01", "2023-01-01")
        """,
        """
        import datetime
        def age(birth, on):
            b, d = datetime.date.fromisoformat(birth), datetime.date.fromisoformat(on)
            if d < b:
                raise ValueError(on)
            years = d.year - b.year
            month, day = b.month, b.day
            if (month, day) == (2, 29) and not (d.year % 4 == 0 and (d.year % 100 != 0 or d.year % 400 == 0)):
                day = 28
            if (d.month, d.day) < (month, day):
                years -= 1
            return years
        """,
    ),
    "pagination": (
        [("pages", "total: int, current: int, window: int", "list",
          "Page links to show: the first and last page, the pages within `window` of `current`, and '...' "
          "for a gap of two or more pages (a gap of one page shows that page). ValueError if `current` is "
          "not between 1 and `total`.")],
        """
        self.assertEqual(pages(10, 5, 1), [1, "...", 4, 5, 6, "...", 10])
        self.assertEqual(pages(5, 3, 1), [1, 2, 3, 4, 5])
        self.assertEqual(pages(10, 1, 1), [1, 2, "...", 10])
        self.assertEqual(pages(1, 1, 2), [1])
        self.assertEqual(pages(7, 4, 1), [1, 2, 3, 4, 5, 6, 7])
        with self.assertRaises(ValueError):
            pages(5, 6, 1)
        """,
        """
        def pages(total, current, window):
            if not 1 <= current <= total:
                raise ValueError(current)
            shown = sorted({1, total} | set(range(max(1, current - window), min(total, current + window) + 1)))
            out = []
            for p in shown:
                if out and p - out[-1] == 2:
                    out.append(p - 1)
                elif out and p - out[-1] > 2:
                    out.append("...")
                out.append(p)
            return out
        """,
    ),
    "income": (
        [("tax", "income: float, brackets: list", "float",
          "Progressive tax: `brackets` is a list of (upper limit or None for the top bracket, rate in "
          "percent), ascending; each slice of income is taxed at its bracket's rate. Rounded to 2 decimals. "
          "ValueError for negative income.")],
        """
        brackets = [(10000, 0), (40000, 20), (None, 40)]
        self.assertEqual(tax(5000, brackets), 0)
        self.assertEqual(tax(10000, brackets), 0)
        self.assertEqual(tax(25000, brackets), 3000.0)
        self.assertEqual(tax(50000, brackets), 10000.0)
        with self.assertRaises(ValueError):
            tax(-1, brackets)
        """,
        """
        def tax(income, brackets):
            if income < 0:
                raise ValueError(income)
            total, lower = 0.0, 0
            for upper, rate in brackets:
                top = income if upper is None else min(income, upper)
                if top > lower:
                    total += (top - lower) * rate / 100
                if upper is None or income <= upper:
                    break
                lower = upper
            return round(total, 2)
        """,
    ),
    "loan": (
        [("payment", "principal: float, annual_rate: float, months: int", "float",
          "Monthly payment of an amortized loan (annual rate in percent, compounded monthly), rounded to "
          "2 decimals; with a 0 rate, principal / months. ValueError if months < 1.")],
        """
        self.assertEqual(payment(1000, 0, 4), 250.0)
        self.assertEqual(payment(200000, 6, 360), 1199.10)
        self.assertEqual(payment(10000, 12, 12), 888.49)
        with self.assertRaises(ValueError):
            payment(1000, 5, 0)
        """,
        """
        def payment(principal, annual_rate, months):
            if months < 1:
                raise ValueError(months)
            r = annual_rate / 1200
            if r == 0:
                return round(principal / months, 2)
            return round(principal * r / (1 - (1 + r) ** -months), 2)
        """,
    ),
    "allocate": (
        [("allocate", "total: int, ratios: list", "list",
          "Split an integer total in proportion to `ratios`: everyone gets the floor of their exact share, "
          "and the units left over go one each to the largest fractional remainders (the earliest on "
          "ties). ValueError if there are no ratios or they sum to 0.")],
        """
        self.assertEqual(allocate(100, [1, 1, 1]), [34, 33, 33])
        self.assertEqual(allocate(10, [1, 2]), [3, 7])
        self.assertEqual(allocate(5, [0, 1]), [0, 5])
        for bad in ([], [0, 0]):
            with self.assertRaises(ValueError):
                allocate(5, bad)
        """,
        """
        def allocate(total, ratios):
            s = sum(ratios)
            if not ratios or s == 0:
                raise ValueError(ratios)
            shares = [total * r // s for r in ratios]
            rems = [(-(total * r % s), i) for i, r in enumerate(ratios)]
            for _, i in sorted(rems)[: total - sum(shares)]:
                shares[i] += 1
            return shares
        """,
    ),
    "serial": (
        [("next_id", "current: str", "str",
          "Increment the number at the end of an id, keeping its zero padding and growing it when needed "
          "('INV-0099' -> 'INV-0100', 'INV-9999' -> 'INV-10000'). ValueError if it does not end in a digit.")],
        """
        self.assertEqual(next_id("INV-0099"), "INV-0100")
        self.assertEqual(next_id("INV-9999"), "INV-10000")
        self.assertEqual(next_id("A7"), "A8")
        self.assertEqual(next_id("X-009"), "X-010")
        with self.assertRaises(ValueError):
            next_id("INV-")
        """,
        """
        import re
        def next_id(current):
            m = re.fullmatch(r"(.*?)(\\d+)", current)
            if not m:
                raise ValueError(current)
            digits = m[2]
            return m[1] + str(int(digits) + 1).zfill(len(digits))
        """,
    ),
    "email": (
        [("normalize", "address: str", "str",
          "Trim and lowercase an address; for gmail.com and googlemail.com, drop dots and anything from a "
          "'+' in the local part and use gmail.com. ValueError unless there is exactly one '@' with text on "
          "both sides.")],
        """
        self.assertEqual(normalize(" John.Doe+news@GMAIL.com "), "johndoe@gmail.com")
        self.assertEqual(normalize("a.b+c@example.com"), "a.b+c@example.com")
        self.assertEqual(normalize("x@googlemail.com"), "x@gmail.com")
        for bad in ("nope", "a@b@c", "@x.com", "x@"):
            with self.assertRaises(ValueError):
                normalize(bad)
        """,
        """
        def normalize(address):
            a = address.strip().lower()
            if a.count("@") != 1:
                raise ValueError(address)
            local, domain = a.split("@")
            if not local or not domain:
                raise ValueError(address)
            if domain in ("gmail.com", "googlemail.com"):
                local, domain = local.split("+")[0].replace(".", ""), "gmail.com"
            return f"{local}@{domain}"
        """,
    ),
    "paths": (
        [("normalize", "path: str", "str",
          "Normalize a POSIX path: collapse repeated slashes, drop '.', resolve '..' (at the root of an "
          "absolute path it is dropped; at the start of a relative path it is kept), no trailing slash "
          "except for '/'; '' becomes '.'.")],
        """
        self.assertEqual(normalize("/a/./b/../c/"), "/a/c")
        self.assertEqual(normalize("a//b"), "a/b")
        self.assertEqual(normalize("/../x"), "/x")
        self.assertEqual(normalize("../a/.."), "..")
        self.assertEqual(normalize(""), ".")
        self.assertEqual(normalize("/"), "/")
        """,
        """
        def normalize(path):
            absolute = path.startswith("/")
            out = []
            for part in path.split("/"):
                if part in ("", "."):
                    continue
                if part == "..":
                    if out and out[-1] != "..":
                        out.pop()
                    elif not absolute:
                        out.append("..")
                else:
                    out.append(part)
            text = "/".join(out)
            return "/" + text if absolute else text or "."
        """,
    ),
    "grades": (
        [("letter", "score: int", "str",
          "Letter grade for 0-100: A from 90, B from 80, C from 70, D from 60, otherwise F. For A to D, a "
          "last digit of 7-9 adds '+' and 0-2 adds '-', except that there is no A+ and 100 is 'A'; F has "
          "no sign. ValueError outside 0-100.")],
        """
        self.assertEqual(letter(95), "A")
        self.assertEqual(letter(92), "A-")
        self.assertEqual(letter(98), "A")
        self.assertEqual(letter(100), "A")
        self.assertEqual(letter(88), "B+")
        self.assertEqual(letter(80), "B-")
        self.assertEqual(letter(75), "C")
        self.assertEqual(letter(59), "F")
        with self.assertRaises(ValueError):
            letter(101)
        """,
        """
        def letter(score):
            if not 0 <= score <= 100:
                raise ValueError(score)
            if score == 100:
                return "A"
            if score < 60:
                return "F"
            grade = "DCBA"[min(score // 10, 9) - 6]
            last = score % 10
            if last >= 7 and grade != "A":
                return grade + "+"
            if last <= 2:
                return grade + "-"
            return grade
        """,
    ),
    "shipping": (
        [("cost", "weight_kg: float, zone: str", "float",
          "Shipping cost: zone 'domestic', 'eu', or 'world' costs 5.00, 9.00, or 15.00 for the first kg and "
          "1.50, 2.50, or 4.00 for every started kg after it. Rounded to 2 decimals. ValueError for an "
          "unknown zone or a weight not above 0 and up to 30.")],
        """
        self.assertEqual(cost(1, "domestic"), 5.0)
        self.assertEqual(cost(1.2, "domestic"), 6.5)
        self.assertEqual(cost(3, "eu"), 14.0)
        self.assertEqual(cost(0.5, "world"), 15.0)
        with self.assertRaises(ValueError):
            cost(31, "eu")
        with self.assertRaises(ValueError):
            cost(1, "mars")
        """,
        """
        import math
        def cost(weight_kg, zone):
            rates = {"domestic": (5.0, 1.5), "eu": (9.0, 2.5), "world": (15.0, 4.0)}
            if zone not in rates or not 0 < weight_kg <= 30:
                raise ValueError((weight_kg, zone))
            base, extra = rates[zone]
            return round(base + max(0, math.ceil(weight_kg - 1)) * extra, 2)
        """,
    ),
    "coupons": (
        [("apply", "subtotal: int, coupons: list", "int",
          "Price in cents after coupons: of the percentage coupons ('10%') only the largest applies, first, "
          "its discount rounded half up to the cent; then every fixed coupon (a number of cents, '500') is "
          "subtracted; never below 0. ValueError for any other coupon.")],
        """
        self.assertEqual(apply(10000, ["10%"]), 9000)
        self.assertEqual(apply(10000, ["10%", "20%"]), 8000)
        self.assertEqual(apply(10000, ["20%", "500", "300"]), 7200)
        self.assertEqual(apply(1000, ["2000"]), 0)
        self.assertEqual(apply(999, ["50%"]), 499)
        with self.assertRaises(ValueError):
            apply(100, ["abc"])
        """,
        """
        def apply(subtotal, coupons):
            percents, fixed = [], 0
            for c in coupons:
                if c.endswith("%") and c[:-1].isdigit():
                    percents.append(int(c[:-1]))
                elif c.isdigit():
                    fixed += int(c)
                else:
                    raise ValueError(c)
            price = subtotal
            if percents:
                price -= (subtotal * max(percents) + 50) // 100
            return max(0, price - fixed)
        """,
    ),
    "freeslots": (
        [("free", "busy: list, start: str, end: str, min_minutes: int", "list",
          "Free slots of at least `min_minutes` between `start` and `end` ('HH:MM'), given busy slots "
          "'HH:MM-HH:MM' in any order and possibly overlapping, as 'HH:MM-HH:MM' strings in order.")],
        """
        self.assertEqual(
            free(["09:00-10:00", "12:00-13:00"], "08:00", "17:00", 30),
            ["08:00-09:00", "10:00-12:00", "13:00-17:00"],
        )
        self.assertEqual(free(["09:00-10:30", "10:00-11:00"], "09:00", "12:00", 15), ["11:00-12:00"])
        self.assertEqual(free([], "09:00", "10:00", 60), ["09:00-10:00"])
        self.assertEqual(free(["09:10-09:50"], "09:00", "10:00", 15), [])
        """,
        """
        def _m(t):
            h, m = t.split(":")
            return int(h) * 60 + int(m)
        def _t(m):
            return f"{m // 60:02d}:{m % 60:02d}"
        def free(busy, start, end, min_minutes):
            spans = sorted((_m(a), _m(b)) for a, b in (s.split("-") for s in busy))
            out, cur, stop = [], _m(start), _m(end)
            for a, b in spans:
                if a - cur >= min_minutes:
                    out.append(f"{_t(cur)}-{_t(min(a, stop))}")
                cur = max(cur, b)
            if stop - cur >= min_minutes:
                out.append(f"{_t(cur)}-{_t(stop)}")
            return out
        """,
    ),
    "histogram": (
        [("render", "counts: dict, width: int", "str",
          "One line per key in sorted order: '<key padded to the longest key> | <bar> <count>', every line "
          "ending in a newline. The largest count gets `width` '#'; the others are scaled and rounded half "
          "up.")],
        """
        self.assertEqual(render({"b": 2, "a": 4}, 8), "a | ######## 4\\nb | #### 2\\n")
        self.assertEqual(render({"x": 1, "long": 3}, 6), "long | ###### 3\\nx    | ## 1\\n")
        self.assertEqual(render({}, 5), "")
        self.assertEqual(render({"z": 0, "y": 5}, 5), "y | ##### 5\\nz |  0\\n")
        """,
        """
        def render(counts, width):
            if not counts:
                return ""
            top = max(counts.values()) or 1
            pad = max(len(k) for k in counts)
            return "".join(
                f"{k.ljust(pad)} | {'#' * ((2 * counts[k] * width + top) // (2 * top))} {counts[k]}\\n"
                for k in sorted(counts)
            )
        """,
    ),
    "morse": (
        [("encode", "text: str", "str",
          "Morse code for letters and digits (case ignored): letters separated by a space, words by ' / '. "
          "ValueError for any other character."),
         ("decode", "code: str", "str", "The inverse, in uppercase; ValueError for an unknown code.")],
        """
        self.assertEqual(encode("SOS"), "... --- ...")
        self.assertEqual(encode("hi there"), ".... .. / - .... . .-. .")
        self.assertEqual(encode("a1"), ".- .----")
        self.assertEqual(decode("... --- ..."), "SOS")
        self.assertEqual(decode(".... .. / - .... . .-. ."), "HI THERE")
        with self.assertRaises(ValueError):
            encode("@")
        """,
        """
        CODE = dict(zip("ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789", (
            ".- -... -.-. -.. . ..-. --. .... .. .--- -.- .-.. -- -. --- .--. --.- .-. ... - ..- ...- .-- -..- -.-- --.. "
            "----- .---- ..--- ...-- ....- ..... -.... --... ---.. ----.").split()))
        BACK = {v: k for k, v in CODE.items()}
        def encode(text):
            try:
                return " / ".join(" ".join(CODE[c] for c in w) for w in text.upper().split())
            except KeyError as e:
                raise ValueError(e) from None
        def decode(code):
            try:
                return " ".join("".join(BACK[c] for c in w.split()) for w in code.split(" / "))
            except KeyError as e:
                raise ValueError(e) from None
        """,
    ),
    "nato": (
        [("spell", "text: str", "str",
          "Spell letters with the NATO alphabet (Alfa, Bravo, ..., X-ray, Yankee, Zulu) and digits as "
          "English words (Zero to Nine), separated by spaces; spaces in the input are skipped; ValueError "
          "for any other character.")],
        """
        self.assertEqual(spell("abc"), "Alfa Bravo Charlie")
        self.assertEqual(spell("Hi 2"), "Hotel India Two")
        self.assertEqual(spell("xj"), "X-ray Juliett")
        with self.assertRaises(ValueError):
            spell("a!")
        """,
        """
        WORDS = ("Alfa Bravo Charlie Delta Echo Foxtrot Golf Hotel India Juliett Kilo Lima Mike November "
                 "Oscar Papa Quebec Romeo Sierra Tango Uniform Victor Whiskey X-ray Yankee Zulu").split()
        DIGITS = "Zero One Two Three Four Five Six Seven Eight Nine".split()
        def spell(text):
            out = []
            for c in text:
                if c == " ":
                    continue
                if c.isascii() and c.isalpha():
                    out.append(WORDS[ord(c.upper()) - 65])
                elif c.isdigit():
                    out.append(DIGITS[int(c)])
                else:
                    raise ValueError(c)
            return " ".join(out)
        """,
    ),
    "numwords": (
        [("say", "n: int", "str",
          "English words for 0 to 999999, tens and units joined by a hyphen, no 'and' ('one hundred "
          "twenty-three'). ValueError outside that range.")],
        """
        self.assertEqual(say(0), "zero")
        self.assertEqual(say(13), "thirteen")
        self.assertEqual(say(21), "twenty-one")
        self.assertEqual(say(100), "one hundred")
        self.assertEqual(say(123), "one hundred twenty-three")
        self.assertEqual(say(1005), "one thousand five")
        self.assertEqual(say(999999), "nine hundred ninety-nine thousand nine hundred ninety-nine")
        with self.assertRaises(ValueError):
            say(1000000)
        """,
        """
        ONES = ("zero one two three four five six seven eight nine ten eleven twelve thirteen fourteen "
                "fifteen sixteen seventeen eighteen nineteen").split()
        TENS = "_ _ twenty thirty forty fifty sixty seventy eighty ninety".split()
        def _under_1000(n):
            parts = []
            if n >= 100:
                parts.append(ONES[n // 100] + " hundred")
                n %= 100
            if n >= 20:
                parts.append(TENS[n // 10] + ("-" + ONES[n % 10] if n % 10 else ""))
            elif n:
                parts.append(ONES[n])
            return " ".join(parts)
        def say(n):
            if not 0 <= n <= 999999:
                raise ValueError(n)
            if n == 0:
                return "zero"
            thousands, rest = divmod(n, 1000)
            parts = [_under_1000(thousands) + " thousand"] if thousands else []
            if rest:
                parts.append(_under_1000(rest))
            return " ".join(parts)
        """,
    ),
    "ordinal": (
        [("ordinal", "n: int", "str", "'1st', '2nd', '3rd', '4th', '11th', '12th', '13th', '21st', ...; ValueError for a negative n.")],
        """
        for n, text in [(0, "0th"), (1, "1st"), (2, "2nd"), (3, "3rd"), (4, "4th"), (11, "11th"),
                        (12, "12th"), (13, "13th"), (21, "21st"), (102, "102nd"), (111, "111th")]:
            self.assertEqual(ordinal(n), text)
        with self.assertRaises(ValueError):
            ordinal(-1)
        """,
        """
        def ordinal(n):
            if n < 0:
                raise ValueError(n)
            if n % 100 in (11, 12, 13):
                return f"{n}th"
            return f"{n}" + {1: "st", 2: "nd", 3: "rd"}.get(n % 10, "th")
        """,
    ),
    "scrabble": (
        [("score", "word: str, word_multiplier: int = 1", "int",
          "Scrabble score (case ignored): AEIOULNRST 1, DG 2, BCMP 3, FHVWY 4, K 5, JX 8, QZ 10, times "
          "`word_multiplier`. ValueError for a non-letter.")],
        """
        self.assertEqual(score("cabbage"), 14)
        self.assertEqual(score("Quiz"), 22)
        self.assertEqual(score(""), 0)
        self.assertEqual(score("ab", 3), 12)
        with self.assertRaises(ValueError):
            score("a1")
        """,
        """
        VALUES = {c: v for letters, v in [("AEIOULNRST", 1), ("DG", 2), ("BCMP", 3), ("FHVWY", 4),
                                          ("K", 5), ("JX", 8), ("QZ", 10)] for c in letters}
        def score(word, word_multiplier=1):
            try:
                return sum(VALUES[c] for c in word.upper()) * word_multiplier
            except KeyError as e:
                raise ValueError(e) from None
        """,
    ),
    "isogram": (
        [("is_isogram", "text: str", "bool", "Whether no letter repeats (case ignored); spaces and hyphens are ignored.")],
        """
        self.assertTrue(is_isogram("lumberjacks"))
        self.assertTrue(is_isogram("six-year-old"))
        self.assertTrue(is_isogram(""))
        self.assertFalse(is_isogram("isograms"))
        self.assertFalse(is_isogram("Alphabet"))
        """,
        """
        def is_isogram(text):
            letters = [c.lower() for c in text if c not in " -"]
            return len(letters) == len(set(letters))
        """,
    ),
    "pangram": (
        [("missing", "text: str", "str", "The letters a-z that do not occur in the text (case ignored), in alphabetical order.")],
        """
        self.assertEqual(missing("The quick brown fox jumps over the lazy dog"), "")
        self.assertEqual(missing("abc"), "defghijklmnopqrstuvwxyz")
        self.assertEqual(missing(""), "abcdefghijklmnopqrstuvwxyz")
        """,
        """
        import string
        def missing(text):
            seen = set(text.lower())
            return "".join(c for c in string.ascii_lowercase if c not in seen)
        """,
    ),
    "acronym": (
        [("make", "phrase: str", "str",
          "Uppercase first letters of the words; words are separated by spaces and hyphens, and other "
          "punctuation (such as underscores) is ignored; an apostrophe stays inside its word.")],
        """
        self.assertEqual(make("Portable Network Graphics"), "PNG")
        self.assertEqual(make("Complementary metal-oxide semiconductor"), "CMOS")
        self.assertEqual(make("The Road _Not_ Taken"), "TRNT")
        self.assertEqual(make("Halley's Comet"), "HC")
        """,
        """
        import re
        def make(phrase):
            return "".join(w[0].upper() for w in re.findall(r"[A-Za-z][A-Za-z']*", phrase.replace("-", " ")))
        """,
    ),
    "series": (
        [("largest_product", "digits: str, span: int", "int",
          "The largest product of `span` adjacent digits; 1 for span 0. ValueError for a negative span, a "
          "span longer than the digits, or a non-digit.")],
        """
        self.assertEqual(largest_product("63915", 3), 162)
        self.assertEqual(largest_product("1027839564", 3), 270)
        self.assertEqual(largest_product("", 0), 1)
        for digits, span in (("12", 3), ("1a", 1), ("123", -1)):
            with self.assertRaises(ValueError):
                largest_product(digits, span)
        """,
        """
        import math
        def largest_product(digits, span):
            if span < 0 or span > len(digits) or not all(c.isdigit() for c in digits):
                raise ValueError((digits, span))
            return max(math.prod(int(c) for c in digits[i:i + span]) for i in range(len(digits) - span + 1))
        """,
    ),
}

MORE["wordfreq"] = (
    [("top", "text: str, n: int", "list",
      "The n most frequent words (lowercased runs of letters and apostrophes) as (word, count) pairs, "
      "most frequent first, ties in alphabetical order.")],
    """
    self.assertEqual(top("the cat and the hat and the bat", 2), [("the", 3), ("and", 2)])
    self.assertEqual(top("", 3), [])
    self.assertEqual(top("Don't stop, don't", 1), [("don't", 2)])
    self.assertEqual(top("b a", 5), [("a", 1), ("b", 1)])
    """,
    """
    import collections, re
    def top(text, n):
        counts = collections.Counter(re.findall(r"[a-z']+", text.lower()))
        return sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))[:n]
    """,
)
