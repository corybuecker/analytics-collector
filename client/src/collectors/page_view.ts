class PageView {
    constructor(endpoint: URL) {
        document?.addEventListener("DOMContentLoaded", () => {
            const path = window?.location?.pathname;

            path && navigator.sendBeacon(endpoint.toString(), JSON.stringify({ entity: "page", action: "view", path }))
        });
    }
}

export default PageView
