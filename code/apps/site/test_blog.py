import unittest

import blog


class Headings(unittest.TestCase):
    def test_unsupported_headings_fail_with_a_line_number(self):
        for level in range(4, 7):
            with self.subTest(level=level), self.assertRaisesRegex(SystemExit, r"line 3: use ## or ###"):
                blog.render("Intro\n\n" + "#" * level + " Detail\n\nBody")

    def test_supported_headings_render_and_count_words(self):
        rendered, words = blog.render("## Section\n\n### Detail\n\nBody")
        self.assertIn("<h2>Section</h2>", rendered)
        self.assertIn("<h3>Detail</h3>", rendered)
        self.assertEqual(words, 3)


if __name__ == "__main__":
    unittest.main()
