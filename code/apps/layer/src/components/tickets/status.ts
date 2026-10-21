export function statusClass(category: string) {
  if (category === "done") return "ui-status-ok";
  if (category === "indeterminate") return "ui-status-info";
  return "";
}
