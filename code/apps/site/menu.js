// The Products menu is a <details>, so it opens without this script.
// This closes it the way a menu closes: Escape, or a click outside it.
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
