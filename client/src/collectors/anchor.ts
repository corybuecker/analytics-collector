class Anchor {
    constructor(endpoint: URL) {
        document?.addEventListener("DOMContentLoaded", () => {
            const anchors = document.querySelectorAll("a")

            anchors.forEach((anchor: HTMLAnchorElement) => {
                console.log(anchor)
                anchor.addEventListener("click", (_event: MouseEvent) => {
                    const href = anchor.getAttribute("href")

                    if (href) {
                        navigator.sendBeacon(endpoint.toString(), JSON.stringify({ entity: "anchor", action: "click", path: href }))
                    }
                });
            });
        });
    }
}

export default Anchor
