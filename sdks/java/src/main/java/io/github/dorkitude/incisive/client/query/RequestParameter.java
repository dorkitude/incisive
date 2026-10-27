package io.github.dorkitude.incisive.client.query;

/** Something that serializes itself into request arguments. */
abstract class RequestParameter<ArgsT> {
    abstract ArgsT constructParameterRequestArgs();
}
