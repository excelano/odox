# The package identity Partner Center assigned, less the one value that is not
# this repository's to hold.
#
# `Publisher` is not here. It is the X.500 string Partner Center assigns per
# account, identical for every Excelano product, and it comes from the
# organisation variable STORE_PUBLISHER, which `windows.yml` passes to
# `build-msix.ps1` and which a Windows machine sets in its own environment.
# Everything below is public: the names and the store ids are on the listing
# pages and the package family names are in every package the Store
# distributes.
#
# One entry per application, keyed by binary. Three reservations, three
# listings, one account.
#
# Author: David M. Anderson
# Built with AI assistance (Claude, Anthropic)
@{
    # Package/Properties/PublisherDisplayName
    PublisherDisplayName = 'Excelano'

    Applications = @{
        xodt = @{
            Name = 'Excelano.OdoxText'
            PackageFamilyName = 'Excelano.OdoxText_nbxmgv0sk86m4'
            StoreId = '9N90RJZDXQZC'
        }
        xods = @{
            Name = 'Excelano.OdoxGrid'
            PackageFamilyName = 'Excelano.OdoxGrid_nbxmgv0sk86m4'
            StoreId = '9MWKG7FFB4DZ'
        }
        xodp = @{
            Name = 'Excelano.OdoxDeck'
            PackageFamilyName = 'Excelano.OdoxDeck_nbxmgv0sk86m4'
            StoreId = '9P8ZDZPQFQTF'
        }
    }
}
