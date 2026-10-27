package org.kanonak.codec.jackson;

import com.fasterxml.jackson.annotation.JsonIgnore;
import com.fasterxml.jackson.annotation.JsonInclude;
import com.fasterxml.jackson.annotation.JsonProperty;
import java.util.List;
import java.util.Map;

/**
 * {@link org.kanonak.codec.KanonakNode}'s accessors, each named by its envelope
 * key. An envelope key that's absent is left out on write, as
 * {@code Codec.serialize} leaves it out; the generated class's own fields keep
 * whatever inclusion it gives them.
 */
abstract class KanonakNodeMixIn {
    @JsonProperty("$id") @JsonInclude(JsonInclude.Include.NON_NULL) abstract String getId();
    @JsonProperty("$id") abstract void setId(String id);

    @JsonProperty("$type") @JsonInclude(JsonInclude.Include.NON_NULL) abstract String getTypeUri();
    @JsonProperty("$type") abstract void setTypeUri(String typeUri);

    @JsonProperty("$types") @JsonInclude(JsonInclude.Include.NON_NULL) abstract List<String> getTypes();
    @JsonProperty("$types") abstract void setTypes(List<String> types);

    @JsonProperty("$name") @JsonInclude(JsonInclude.Include.NON_NULL) abstract String getName();
    @JsonProperty("$name") abstract void setName(String name);

    @JsonProperty("$contentHash") @JsonInclude(JsonInclude.Include.NON_NULL) abstract String getPackageContentHash();
    @JsonProperty("$contentHash") abstract void setPackageContentHash(String packageContentHash);

    @JsonProperty("$version") @JsonInclude(JsonInclude.Include.NON_NULL) abstract String getPackageVersion();
    @JsonProperty("$version") abstract void setPackageVersion(String packageVersion);

    @JsonIgnore abstract Map<String, Object> getExtra();
    @JsonIgnore abstract void setExtra(Map<String, Object> extra);
}
