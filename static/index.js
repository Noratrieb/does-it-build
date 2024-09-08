class Table {
  constructor(data, tableElemId, filterElemId, filterFailedElemId) {
    this.data = data;
    this.elem = document.getElementById(tableElemId);

    document.getElementById(filterElemId).addEventListener("input", (e) => {
      this.filter.search = e.target.value;
      this.render();
    });
    document
      .getElementById(filterFailedElemId)
      .addEventListener("input", (e) => {
        this.filter.filterFailed = e.target.checked;
        this.render();
      });

    this.filter = {
      search: "",
      filterFailed: false,
    };
  }

  update(data) {
    this.data = data;
  }

  render() {
    const allTargets = new Set();
    const allNightlies = new Set();

    const nightlyInfos = new Map();

    // Targets that have, at some point, errored
    const targetsWithErrors = new Set();

    // Whether a nightly is completely broken.
    // These are still filtered out when filter failed is selected.
    const isNightlyBroken = new Map();

    // The first pass over the data, to find nightlies that are broken.
    for (const info of this.data) {
      if (!isNightlyBroken.has(info.nightly)) {
        // Assume that a nightly is broken until proven otherwise.
        isNightlyBroken.set(info.nightly, true);
      }
      if (info.status == "pass") {
        // This nightly has built something, so it's clearly not broken :).
        isNightlyBroken.set(info.nightly, false);
      }
    }

    // Second pass over the data, group by nightly and prepare data for filter.
    for (const info of this.data) {
      allNightlies.add(info.nightly);

      if (!info.target.includes(this.filter.search)) {
        continue;
      }

      if (info.status === "error" && !isNightlyBroken.get(info.nightly)) {
        targetsWithErrors.add(info.target);
      }

      allTargets.add(info.target);
      if (!nightlyInfos.has(info.nightly)) {
        nightlyInfos.set(info.nightly, new Map());
      }
      nightlyInfos.get(info.nightly).set(info.target, info);
    }

    const nightlies = Array.from(allNightlies);
    nightlies.sort();
    nightlies.reverse();
    const targets = Array.from(allTargets);
    targets.sort();

    const header = document.createElement("tr");
    const headerNightly = document.createElement("th");
    headerNightly.innerText = "nightly";
    header.appendChild(headerNightly);
    targets.forEach((target) => {
      if (this.filter.filterFailed && !targetsWithErrors.has(target)) {
        return;
      }
      const elem = document.createElement("th");
      elem.innerText = target;
      header.appendChild(elem);
    });

    const rows = nightlies.map((nightly) => {
      const tr = document.createElement("tr");

      const nightlyCol = document.createElement("td");
      nightlyCol.innerText = nightly;
      tr.appendChild(nightlyCol);

      const info = nightlyInfos.get(nightly) ?? new Map();

      for (const target of targets) {
        if (this.filter.filterFailed && !targetsWithErrors.has(target)) {
          continue;
        }

        const td = document.createElement("td");
        const targetInfo = info.get(target);

        if (targetInfo) {
          const a = document.createElement("a");
          a.classList.add("build-info-a");
          a.href = `build?nightly=${encodeURIComponent(
            nightly
          )}&target=${encodeURIComponent(target)}&mode=${encodeURIComponent(
            targetInfo.mode
          )}`;
          a.innerText = targetInfo.status;
          td.appendChild(a);
          td.classList.add(targetInfo.status);
        } else {
          td.innerText = "";
          td.classList.add("missing");
        }
        tr.appendChild(td);
      }

      return tr;
    });
    this.elem.replaceChildren(header, ...rows);
  }
}

const coreTable = new Table(
  [],
  "target-state",
  "target-filter",
  "target-filter-failed"
);
const miriTable = new Table(
  [],
  "target-state-miri",
  "target-filter-miri",
  "target-filter-failed-miri"
);

function fetchTargets() {
  fetch("target-state")
    .then((body) => body.json())
    .then((body) => {
      const core = body.filter((info) => info.mode === "core");
      const miri = body.filter((info) => info.mode === "miri-std");
      coreTable.update(core);
      miriTable.update(miri);
      coreTable.render();
      miriTable.render();
    });
}

// Initial fetch
fetchTargets();
