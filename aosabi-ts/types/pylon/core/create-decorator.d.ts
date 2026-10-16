export declare function createDecorator(callback: (...args: any[]) => Promise<void>): {
    <T extends (...args: any[]) => any>(target: Object, propertyKey: string | symbol): void;
    <T>(fn: T): T;
};
