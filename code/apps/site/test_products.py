import unittest

import products


class Pages(unittest.TestCase):
    def test_homepage_menu_matches_the_products(self):
        home = (products.SITE / "index.html").read_text()
        self.assertIn(products.menu(0), home)

    def test_every_capture_exists_and_has_alt_text(self):
        for product in products.products():
            blocks = [product.data["hero"], *product.data.get("feature", [])]
            for capture in (c for block in blocks for c in block["captures"]):
                with self.subTest(product=product.slug, capture=capture["src"]):
                    self.assertTrue((products.CAPTURES / capture["src"]).is_file())
                    self.assertTrue(capture["alt"].strip())

    def test_every_page_renders_without_placeholders(self):
        template = products.Template((products.TEMPLATES / "product.html").read_text())
        for product in products.products():
            with self.subTest(product=product.slug):
                self.assertNotRegex(products.page(product, template), r"\$\w")


if __name__ == "__main__":
    unittest.main()
