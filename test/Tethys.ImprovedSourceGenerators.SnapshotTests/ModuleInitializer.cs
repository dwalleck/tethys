using System.Runtime.CompilerServices;
using System.Text;

namespace Tethys.ImprovedSourceGenerators.SnapshotTests;

public static class ModuleInitializer
{
    [ModuleInitializer]
    public static void Init()
    {
        // Configure Verify settings
        VerifySourceGenerators.Initialize();
    }
}