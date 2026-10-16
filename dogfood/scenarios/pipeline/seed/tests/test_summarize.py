import unittest

from pipeline._stub import is_stub
from pipeline.slug import slugify
from pipeline.stats import top_words
from pipeline.summary import summarize
from pipeline.text import normalize
from pipeline.tokens import tokenize


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (summarize, slugify, top_words, tokenize, normalize,)):
            self.skipTest("not implemented yet (or a dependency is not)")

    def test_summarize(self):
        text = "Café Notes\nthe cat and the hat\n"
        self.assertEqual(
            summarize(text),
            {"slug": "cafe-notes", "words": 7, "top": [("the", 2), ("and", 1), ("cafe", 1)]},
        )
        self.assertEqual(summarize(""), {"slug": "", "words": 0, "top": []})
