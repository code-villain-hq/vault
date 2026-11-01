// The Products and Extras menus are <details>, so they open without this script.
// This closes one the way a menu closes: Escape, or a click outside it.
/** @type {NodeListOf<HTMLDetailsElement>} */
const menus = document.querySelectorAll("details.products-menu");
for (const menu of menus) {
  document.addEventListener("click", (event) => {
    if (menu.open && !(event.target instanceof Node && menu.contains(event.target))) {
      menu.open = false;
    }
  });
  menu.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && menu.open) {
      menu.open = false;
      menu.querySelector("summary")?.focus();
    }
  });
}
