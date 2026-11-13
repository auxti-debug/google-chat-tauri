const list = document.getElementById("downloads");
const rows = new Map();
const number = new Intl.NumberFormat("pt-BR", { maximumFractionDigits: 1 });

function formatSize(bytes) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let unit = 0;
  while (bytes >= 1024 && unit < units.length - 1) {
    bytes /= 1024;
    unit += 1;
  }
  return number.format(bytes) + " " + units[unit];
}

function createRow(id) {
  const item = document.createElement("li");
  const name = document.createElement("span");
  name.className = "name";
  const open = document.createElement("button");
  open.type = "button";
  open.className = "name open";
  open.hidden = true;
  const error = document.createElement("span");
  error.className = "error";
  error.setAttribute("role", "status");
  open.addEventListener("click", async () => {
    error.textContent = "";
    try {
      await window.__TAURI_INTERNALS__.invoke("open_download", { id });
    } catch (message) {
      error.textContent = typeof message === "string" ? message : "Não foi possível abrir o arquivo.";
    }
  });
  const status = document.createElement("span");
  status.className = "status";
  const progress = document.createElement("progress");
  item.append(name, open, status, progress, error);
  const row = { item, name, open, status, progress };
  rows.set(id, row);
  return row;
}

window.renderDownloads = (downloads) => {
  const ids = new Set(downloads.map(({ id }) => id));
  for (const [id, { item }] of rows) {
    if (!ids.has(id)) {
      item.remove();
      rows.delete(id);
    }
  }

  downloads.forEach((download, index) => {
    const row = rows.get(download.id) ?? createRow(download.id);
    if (list.children[index] !== row.item) {
      list.insertBefore(row.item, list.children[index] ?? null);
    }
    row.name.textContent = download.name;
    const canOpen = download.status === "complete";
    row.name.hidden = canOpen;
    row.open.hidden = !canOpen;
    row.open.textContent = canOpen ? download.name : "";
    const received = formatSize(download.received);

    if (download.status === "active") {
      row.progress.hidden = false;
      row.progress.setAttribute("aria-label", download.name);
      if (download.total > 0) {
        const percent = Math.min(100, Math.floor(download.received * 100 / download.total));
        row.progress.max = download.total;
        row.progress.value = download.received;
        row.status.textContent = "Baixando · " + received + " de " + formatSize(download.total) + " · " + percent + "%";
      } else {
        row.progress.removeAttribute("value");
        row.status.textContent = "Baixando · " + received;
      }
    } else {
      row.progress.hidden = true;
      const states = { complete: "Concluído", canceled: "Cancelado", interrupted: "Interrompido" };
      row.status.textContent = states[download.status] + (download.status === "complete" ? " · " + received : "");
    }
  });
};
