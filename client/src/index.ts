import Anchor from "./collectors/anchor";
import PageView from "./collectors/page_view";

type Options = {
  endpoint: URL
}

class AnalyticsCollector {
  private endpoint: URL

  public static initialize(endpoint: string): AnalyticsCollector {
    const endpointURL = URL.parse(endpoint)

    if (!endpointURL) {
      throw new Error("Invalid endpoint URL")
    }

    return new AnalyticsCollector({ endpoint: endpointURL })
  }

  private constructor(options: Options) {
    this.endpoint = options.endpoint
  }

  public start(): void {
    new PageView(this.endpoint)
    new Anchor(this.endpoint)
  }
}

export default AnalyticsCollector
