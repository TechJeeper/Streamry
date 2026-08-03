document.getElementById("year")?.append(String(new Date().getFullYear()));

document.querySelectorAll("[data-download]").forEach((el) => {
  el.addEventListener("click", (event) => {
    if (el.getAttribute("aria-disabled") === "true") {
      event.preventDefault();
    }
  });
});

const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
if (!reduced) {
  const frame = document.querySelector(".app-frame");
  if (frame) {
    window.addEventListener(
      "pointermove",
      (event) => {
        const x = (event.clientX / window.innerWidth - 0.5) * 8;
        const y = (event.clientY / window.innerHeight - 0.5) * 6;
        frame.style.transform = `translate(${x}px, ${y}px)`;
      },
      { passive: true },
    );
  }

  const features = document.querySelectorAll(".feature");
  if (features.length && "IntersectionObserver" in window) {
    const io = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            entry.target.style.transition = "opacity 0.55s ease, transform 0.55s ease";
            entry.target.style.opacity = "1";
            entry.target.style.transform = "none";
            io.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.2 },
    );
    features.forEach((el, i) => {
      el.style.opacity = "0";
      el.style.transform = "translateY(14px)";
      el.style.transitionDelay = `${i * 40}ms`;
      io.observe(el);
    });
  }
}
