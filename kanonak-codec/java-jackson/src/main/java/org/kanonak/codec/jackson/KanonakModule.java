package org.kanonak.codec.jackson;

import com.fasterxml.jackson.databind.module.SimpleModule;
import org.kanonak.codec.KanonakNode;

/**
 * Binds {@link KanonakNode}'s {@code $}-envelope to its JSON keys, so a generated
 * class that Jackson reads or writes keeps its {@code $id}, {@code $type},
 * {@code $types}, {@code $name}, {@code $contentHash} and {@code $version}.
 * Register it with {@code ObjectMapper.registerModule}, or let
 * {@code findAndAddModules} find it. A generated class binds its own fields;
 * {@link KanonakNode#getExtra() extra}, the open-world assertions, is left out
 * of the binding, so an unknown key is Jackson's to refuse or ignore, as the
 * mapper is configured.
 *
 * <p>In an artifact of its own, so {@code kanonak-codec} stays JDK-only.
 */
public final class KanonakModule extends SimpleModule {
    private static final long serialVersionUID = 1L;

    public KanonakModule() {
        super(KanonakModule.class.getName());
        setMixInAnnotation(KanonakNode.class, KanonakNodeMixIn.class);
    }
}
