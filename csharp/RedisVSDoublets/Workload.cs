namespace Comparisons.RedisVSDoublets;

public static class Workload
{
    public static readonly string[] Operations = ["Create", "Update", "Delete", "Each_All", "Each_Identity", "Each_Concrete", "Each_Outgoing", "Each_Incoming"];
    private const ulong Any = ulong.MaxValue;
    private static ulong _sink;
    private static void Visit(Link link) => _sink = unchecked(_sink + link.Id + link.Source + link.Target);

    public static void Exercise(IStore store, int operation, ulong background, ulong links)
    {
        switch (operation)
        {
            case 0:
                for (ulong i = 0; i < links; i++) _sink = store.Create();
                break;
            case 1:
                for (ulong id = 1; id <= links; id++) store.Update(id, id, (id % background) + 1);
                break;
            case 2:
                for (var id = background; id > background - links; id--) store.Delete(id);
                break;
            case 3:
                store.Each(Any, Any, Any, Visit);
                break;
            case 4:
            case 5:
            case 6:
            case 7:
                for (ulong id = 1; id <= links; id++)
                {
                    var query = operation switch
                    {
                        4 => (id, Any, Any),
                        5 => (Any, id, id),
                        6 => (Any, id, Any),
                        _ => (Any, Any, id),
                    };
                    store.Each(query.Item1, query.Item2, query.Item3, Visit);
                }
                break;
            default: throw new ArgumentException("unknown operation");
        }
    }

    public static void Undo(IStore store, int operation, ulong background, ulong links)
    {
        switch (operation)
        {
            case 0:
                for (var id = background + links; id > background; id--) store.Delete(id);
                break;
            case 1:
                for (ulong id = 1; id <= links; id++) store.Update(id, id, id);
                break;
            case 2:
                for (ulong i = 0; i < links; i++) store.Create();
                break;
        }
    }
}
