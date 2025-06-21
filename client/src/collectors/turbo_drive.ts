interface TurboEvent extends Event {
    detail: {
        url: string;
    }
}

class TurboDrive {
    private endpoint: URL
    private appId: string

    constructor(endpoint: URL, appId: string) {
        this.endpoint = endpoint
        this.appId = appId

        const element = document.documentElement
        
        if (element) {
            element.addEventListener('turbo:visit', this.handleVisit.bind(this))
            element.addEventListener('turbo:click', this.handleClick.bind(this))
        }
        
    }

    handleClick(event: Event): void {
        const target = event.target;

        if ((target instanceof HTMLAnchorElement)) {
            const href = target.getAttribute("href")
            navigator.sendBeacon(this.endpoint.toString(), JSON.stringify({ entity: "anchor", action: "click", path: href, appId: this.appId }))
        }
    }

    handleVisit(event: Event): void {
        const turboEvent = event as TurboEvent;
        const url = URL.parse(turboEvent.detail.url)
        navigator.sendBeacon(this.endpoint.toString(), JSON.stringify({ entity: "page", action: "view", path: url?.pathname, appId: this.appId }))
    }
}

export default TurboDrive
