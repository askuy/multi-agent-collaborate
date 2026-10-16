import unittest

from kit._stub import is_stub
from kit.case import to_snake, to_camel


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (to_snake, to_camel,)):
            self.skipTest("not implemented yet")

    def test_case(self):
        self.assertEqual(to_snake("parseHTTPResponse"), "parse_http_response")
        self.assertEqual(to_snake("simple"), "simple")
        self.assertEqual(to_snake("HelloWorld"), "hello_world")
        self.assertEqual(to_camel("parse_http_response"), "parseHttpResponse")
        self.assertEqual(to_camel("hello_world"), "helloWorld")
        self.assertEqual(to_camel("x"), "x")
