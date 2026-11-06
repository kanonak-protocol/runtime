package org.kanonak.codec.jackson;

import com.fasterxml.jackson.databind.JsonDeserializer;
import com.fasterxml.jackson.databind.module.SimpleModule;
import org.kanonak.codec.KanonakNode;
import org.kanonak.codec.Ref;

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
 * <p>It binds {@link Ref} too, a field holding a reference or an embedded node:
 * a reference as {@code {"$ref": uri}}, and an embedded node in place, read as
 * the field's declared {@code Ref<T>}'s {@code T}.
 *
 * <p>In an artifact of its own, so {@code kanonak-codec} stays JDK-only.
 */
public final class KanonakModule extends SimpleModule {
    private static final long serialVersionUID = 1L;

    @SuppressWarnings({"unchecked", "rawtypes"})
    public KanonakModule() {
        super(KanonakModule.class.getName());
        setMixInAnnotation(KanonakNode.class, KanonakNodeMixIn.class);
        addSerializer(new RefBinding.RefSerializer());
        addDeserializer((Class) Ref.class, (JsonDeserializer) new RefBinding.RefDeserializer());
    }
}
