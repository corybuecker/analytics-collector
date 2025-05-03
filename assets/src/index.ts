import AnchorsCollector from "./collectors/anchors"

class Analytics {
    constructor() {
        console.log('Analytics initialized')
    }

    trackEvent(event: string) {
        console.log(`Event tracked: ${event}`)
    }
}

export default Analytics;