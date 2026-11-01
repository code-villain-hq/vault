// The Products and Extras menus are <details>, so they open without this script.
// This closes one the way a menu closes: Escape, or a click outside it.
for (const menu of document.querySelectorAll('.products-menu')) {
  document.addEventListener('click', (event) => {
    if (menu.open && !menu.contains(event.target)) menu.open = false;
  });
  menu.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && menu.open) {
      menu.open = false;
      menu.querySelector('summary').focus();
    }
  });
}
