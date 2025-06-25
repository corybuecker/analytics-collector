import Anchor from "./collectors/anchor";
import PageView from "./collectors/page_view";
import TurboDrive from "./collectors/turbo_drive";

type InitializeOptions = {
  endpoint: URL;
  appId: string;
};

class AnalyticsCollector {
  private endpoint: URL;
  private appId: string;

  public static initialize(
    endpoint: string,
    appId: string,
  ): AnalyticsCollector {
    const endpointURL = URL.parse(endpoint);

    if (!endpointURL) {
      throw new Error("Invalid endpoint URL");
    }

    return new AnalyticsCollector({ endpoint: endpointURL, appId });
  }

  private constructor(options: InitializeOptions) {
    this.endpoint = options.endpoint;
    this.appId = options.appId;
  }

  public start(): void {
    new PageView(this.endpoint, this.appId);
    new Anchor(this.endpoint, this.appId);
    new TurboDrive(this.endpoint, this.appId);
  }
}

export default AnalyticsCollector;
