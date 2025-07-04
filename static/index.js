// Do some fancy number formatting in your locale, so you can't blame me if the numbers look ass.
document.querySelectorAll(".number").forEach((elem) => {
  elem.textContent = new Intl.NumberFormat().format(Number(elem.textContent));
});

// Do some fancy date formatting in your locale, so you can't blame me if the numbers look ass.
document.querySelectorAll(".date").forEach((elem) => {
    console.log("run")
  elem.textContent = new Intl.DateTimeFormat(undefined, {
    dateStyle: "short",
    timeStyle: "short",
  }).format(new Date(elem.textContent));
});
