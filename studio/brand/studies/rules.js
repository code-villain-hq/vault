/* Draft-study interactions. The published tokens and site are unchanged. */
const imageModes = {
  whole: { box: '0 0 900 420', description: 'A whole window establishes where the work happens.', framing: '24px surround · 10px corners · straight on' },
  detail: { box: '560 40 330 300', description: 'A close crop keeps the task title, state, and linked spec.', framing: 'Crop for the task · preserve context' },
};
const imageButtons = document.querySelectorAll('[data-image-view]');
for (const button of imageButtons) {
  button.addEventListener('click', () => {
    const mode = button.dataset.imageView;
    const settings = imageModes[mode];
    for (const other of imageButtons) other.setAttribute('aria-pressed', String(other === button));
    document.querySelector('#rule-art').setAttribute('viewBox', settings.box);
    document.querySelector('.rule-image-stage').dataset.imageMode = mode;
    document.querySelector('#image-description').textContent = settings.description;
    document.querySelector('#image-framing').textContent = settings.framing;
  });
}
const densityModes = {
  app: { title: 'App interface', description: 'Compact groups keep the task and its details in view.', padding: 16, related: 4, group: 12, section: 32 },
  studio: { title: 'Studio page', description: 'More room separates the product story from its supporting details.', padding: 32, related: 8, group: 24, section: 64 },
};
const densityButtons = document.querySelectorAll('button[data-density]');
const narrowStudy = window.matchMedia('(max-width: 720px)');
function applyDensity(mode) {
  const settings = { ...densityModes[mode] };
  if (mode === 'studio' && narrowStudy.matches) {
    settings.padding = 16;
    settings.section = 48;
  }
  for (const button of densityButtons) button.setAttribute('aria-pressed', String(button.dataset.density === mode));
  document.querySelector('.rule-spacing-preview').dataset.density = mode;
  document.querySelector('#density-title').textContent = settings.title;
  document.querySelector('#density-description').textContent = settings.description;
  for (const key of ['padding', 'related', 'group', 'section']) document.querySelector('#space-' + key).textContent = settings[key] + 'px';
}
for (const button of densityButtons) button.addEventListener('click', () => applyDensity(button.dataset.density));
narrowStudy.addEventListener('change', () => applyDensity(document.querySelector('.rule-spacing-preview').dataset.density));
document.querySelector('#show-spacing').addEventListener('change', (event) => {
  document.querySelector('.rule-spacing-preview').dataset.guides = String(event.target.checked);
});
