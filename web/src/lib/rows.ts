import type { Attachment } from 'svelte/attachments';

/**
 * A row of tabs that scrolls sideways when it is wider than the screen:
 * the one picked brought into view, `at` naming it so a new pick brings
 * that one, and the row marked `more-start` or `more-end` while there is
 * more past either edge, which app.css fades.
 */
export function scrollRow(at: unknown): Attachment<HTMLElement> {
  return (row) => {
    void at;
    const edges = () => {
      row.classList.toggle('more-start', row.scrollLeft > 1);
      row.classList.toggle('more-end', row.scrollLeft + row.clientWidth < row.scrollWidth - 1);
    };
    const picked = row.querySelector<HTMLElement>('[aria-current]');
    if (picked && row.scrollWidth > row.clientWidth) {
      const box = row.getBoundingClientRect();
      const it = picked.getBoundingClientRect();
      row.scrollLeft += it.left - box.left - (box.width - it.width) / 2;
    }
    edges();
    row.addEventListener('scroll', edges, { passive: true });
    const sized = new ResizeObserver(edges);
    sized.observe(row);
    return () => {
      row.removeEventListener('scroll', edges);
      sized.disconnect();
    };
  };
}
