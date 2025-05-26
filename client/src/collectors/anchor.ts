class Anchor {
    constructor(endpoint: URL, appId: string) {
        document?.addEventListener("DOMContentLoaded", () => {
            const anchors = document.querySelectorAll("a")

            anchors.forEach((anchor: HTMLAnchorElement) => {
                anchor.addEventListener("click", (_event: MouseEvent) => {
                    const href = anchor.getAttribute("href")

                    if (href) {
                        navigator.sendBeacon(endpoint.toString(), JSON.stringify({ entity: "anchor", action: "click", path: href, appId }))
                    }
                });
            });
        });
    }
}

export default Anchor
