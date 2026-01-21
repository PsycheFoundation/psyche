(() => {
  const darkThemes = ["psyche-dark"];
  const lightThemes = ["psyche-light"];

  const classList = document.getElementsByTagName("html")[0].classList;

  let lastThemeWasLight = true;
  for (const cssClass of classList) {
    if (darkThemes.includes(cssClass)) {
      lastThemeWasLight = false;
      break;
    }
  }

  const forest = {
    darkMode: false,
    background: "#F7F8F7",
    primaryColor: "#EFF2EE",
    primaryTextColor: "#243D26",
    noteBkgColor: "#243D26",
    noteTextColor: "#EFF2EE",
  };
  //
  // .psyche-dark {
  //     --darkgreen: #243D26;
  //     --bright: #FCFDFC;
  //     --brightgreen: #4B9551;
  //     --med-green: #2a4b2d;

  const darkForest = {
    darkMode: true,
    background: "#243D26",
    primaryColor: "#2A4B2D",
    secondaryColor: "#4b970c",
    secondaryTextColor: "#EFF2EE",
    tertiaryColor: "#243D26",
    tertiaryTextColor: "#EFF2EE",
    primaryTextColor: "#FCFDFC",
    noteBkgColor: "#EFF2EE",
    noteTextColor: "#243D26",
    lineColor: "#4B9551",
  };

  const themeVariables = lastThemeWasLight ? forest : darkForest;
  mermaid.initialize({ startOnLoad: true, theme: "base", themeVariables });

  // Simplest way to make mermaid re-render the diagrams in the new theme is via refreshing the page

  for (const darkTheme of darkThemes) {
    document.getElementById(darkTheme).addEventListener("click", () => {
      if (lastThemeWasLight) {
        window.location.reload();
      }
    });
  }

  for (const lightTheme of lightThemes) {
    document.getElementById(lightTheme).addEventListener("click", () => {
      if (!lastThemeWasLight) {
        window.location.reload();
      }
    });
  }
})();
