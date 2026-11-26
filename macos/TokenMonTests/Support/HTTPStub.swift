import Foundation

/// A stub transport: answers every request from `HTTPStub.handler` and records
/// what was sent, so no request leaves the process.
///
/// Tests run one at a time in the app host, so one static handler is enough;
/// call `reset` in `setUp`.
final class HTTPStub: URLProtocol {
    /// A canned reply.
    struct Reply {
        var status = 200
        var headers: [String: String] = [:]
        var body = Data()

        /// A JSON reply from a literal string.
        static func json(_ text: String, status: Int = 200, headers: [String: String] = [:]) -> Reply {
            Reply(status: status, headers: headers, body: Data(text.utf8))
        }
    }

    /// A request as it reached the transport, with its body read out.
    struct Recorded {
        let request: URLRequest
        let body: Data

        /// The body decoded as a JSON object, or nil.
        var json: [String: Any]? {
            (try? JSONSerialization.jsonObject(with: body)) as? [String: Any]
        }
    }

    private static let lock = NSLock()
    private static var handler: (URLRequest) -> Result<Reply, URLError> = { _ in .success(Reply()) }
    private static var recorded: [Recorded] = []

    /// Replaces the handler and clears the recorded requests.
    static func reset(_ handler: @escaping (URLRequest) -> Result<Reply, URLError>) {
        lock.lock()
        defer { lock.unlock() }
        self.handler = handler
        recorded = []
    }

    /// Answers every request with `reply`.
    static func reset(always reply: Reply) {
        reset { _ in .success(reply) }
    }

    /// Requests received since the last `reset`, in order.
    static var requests: [Recorded] {
        lock.lock()
        defer { lock.unlock() }
        return recorded
    }

    /// An ephemeral session whose requests go to this stub.
    static func session() -> URLSession {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [HTTPStub.self]
        return URLSession(configuration: configuration)
    }

    override static func canInit(with request: URLRequest) -> Bool { true }
    override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        guard let url = request.url else { return }
        let body = request.httpBody ?? Self.read(request.httpBodyStream)
        Self.lock.lock()
        Self.recorded.append(Recorded(request: request, body: body))
        let handler = Self.handler
        Self.lock.unlock()
        switch handler(request) {
        case let .success(reply):
            let response = HTTPURLResponse(url: url, statusCode: reply.status, httpVersion: nil, headerFields: reply.headers)
            client?.urlProtocol(self, didReceive: response ?? URLResponse(), cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: reply.body)
            client?.urlProtocolDidFinishLoading(self)
        case let .failure(error):
            client?.urlProtocol(self, didFailWithError: error)
        }
    }

    override func stopLoading() {}

    private static func read(_ stream: InputStream?) -> Data {
        guard let stream else { return Data() }
        stream.open()
        defer { stream.close() }
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)
        while stream.hasBytesAvailable {
            let bytesRead = stream.read(&buffer, maxLength: buffer.count)
            guard bytesRead > 0 else { break }
            data.append(buffer, count: bytesRead)
        }
        return data
    }
}
