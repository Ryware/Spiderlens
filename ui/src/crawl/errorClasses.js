// The engine's wording for each error class (spiderlens_results
// error_class_label). The report labels only the classes without a response,
// so the HTTP ones are worded here too; a class this list does not know shows
// as its id rather than disappearing.
const labels = {
    dns: "DNS lookup failed",
    dns_temporary: "DNS temporarily unavailable",
    connection: "Connection failed",
    timeout: "Timed out",
    tls_invalid: "Invalid TLS certificate",
    redirect_loop: "Redirect loop",
    redirect_problem: "Redirect problem",
    content_problem: "Content problem",
    malformed_url: "Malformed URL",
    host_unavailable: "Host unavailable",
    http_429: "Too many requests (429)",
    http_4xx: "Client error (4xx)",
    http_5xx: "Server error (5xx)",
    http_other: "Unusual status code",
};
/** The label for an error class, the report's own first. */
export function classLabel(cls, report) {
    return report?.failures?.find((f) => f.class === cls)?.label ?? labels[cls] ?? cls;
}
