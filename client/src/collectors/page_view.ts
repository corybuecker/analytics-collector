class PageView {
    constructor(endpoint: URL, appId: string) {
        document?.addEventListener("DOMContentLoaded", () => {
            const path = window?.location?.pathname;
            path && navigator.sendBeacon(endpoint.toString(), JSON.stringify({ entity: "page", action: "view", path, appId }))
        });
    }
}

export default PageView
