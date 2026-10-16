import { FieldNode, GraphQLError, GraphQLErrorExtensions, GraphQLResolveInfo } from 'graphql';
import { Context } from './context';
export interface Resolvers {
    Query: Record<string, any>;
    Mutation?: Record<string, any>;
    Subscription?: Record<string, any>;
}
export declare const getSelectedFields: (info: GraphQLResolveInfo, fieldNodes?: readonly FieldNode[], parentType?: any) => any[];
/**
 * Converts a set of resolvers into a corresponding set of GraphQL resolvers.
 * @param resolvers The original resolvers.
 * @returns The converted GraphQL resolvers.
 */
export declare const resolversToGraphQLResolvers: (resolvers: Resolvers, configureContext?: (context: Context) => Context) => Resolvers;
export declare class ServiceError extends GraphQLError {
    extensions: GraphQLErrorExtensions;
    constructor(message: string, extensions: {
        code: string;
        statusCode: number;
        details?: Record<string, any>;
    }, error?: Error);
}
