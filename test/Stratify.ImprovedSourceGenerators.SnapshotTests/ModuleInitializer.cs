using System.Runtime.CompilerServices;
using System.Text;

namespace Stratify.ImprovedSourceGenerators.SnapshotTests;

public static class ModuleInitializer
{
    [ModuleInitializer]
    public static void Init()
    {
        // Configure Verify settings
        VerifySourceGenerators.Initialize();
    }
}
