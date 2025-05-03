export default class AnchorsCollector {
    constructor() {
        console.log('AnchorsCollector initialized');
    }

    collect() {
        const anchors = document.querySelectorAll('a');
        anchors.forEach(anchor => {
            console.log(`Anchor found: ${anchor.href}`);
        });
    }
}