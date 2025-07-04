// Do some fancy number formatting in your locale, so you can't blame me if the numbers look ass.
document.querySelectorAll(".number").forEach((elem) => {
  elem.textContent = new Intl.NumberFormat().format(Number(elem.textContent));
});
