import { AsyncLocalStorage } from 'async_hooks';
import { FieldNode, GraphQLResolveInfo } from 'graphql';
export interface ExecutionContext {
    info: GraphQLResolveInfo;
    selectedFields: {
        name: string;
        fieldNodes: FieldNode[];
        returnType?: any;
    }[];
}
export declare const executionAsyncContext: AsyncLocalStorage<ExecutionContext>;
export declare const getResolveInfo: () => ExecutionContext;
