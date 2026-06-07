package org.kanonak.codec;

/**
 * The identity of the (data) package being content-addressed — the consumer's
 * package the nodes are assembled into. Used to synthesize the package-wrapper
 * subject {@code <publisher>/<packageName>@<version>/<packageName>}.
 *
 * @param label optional package label (a raw/untyped string statement, as the
 *              parser emits); may be {@code null}.
 */
public record PackageContext(
    String publisher,
    String packageName,
    String version,
    String label
) {
    /** Convenience constructor for a package without a label. */
    public PackageContext(String publisher, String packageName, String version) {
        this(publisher, packageName, version, null);
    }
}
